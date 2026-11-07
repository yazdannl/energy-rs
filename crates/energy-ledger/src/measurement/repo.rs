//! Sample ingestion (an upsert on `(pack_id, ts)`), range reads and the 5-minute rollup.

use std::sync::MutexGuard;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::LedgerError;
use crate::measurement::{Measurement, RollupReport, Sample, Tier, rollup};

/// Repository for the measurement tiers, holding the ledger connection guard.
pub struct Measurements<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> Measurements<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// Upsert one sample: re-ingesting a timestamp replaces the row instead of duplicating it.
    pub fn write(&self, sample: &Sample) -> Result<(), LedgerError> {
        self.conn.execute(
            "INSERT INTO measurements (pack_id, ts, power_w, soc_pct, kind)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(pack_id, ts) DO UPDATE SET
                power_w = excluded.power_w,
                soc_pct = excluded.soc_pct,
                kind = excluded.kind",
            params![
                sample.pack_id,
                sample.ts,
                sample.power_w,
                sample.soc_pct,
                sample.kind.as_str(),
            ],
        )?;
        Ok(())
    }

    /// Upsert a batch in one transaction (a poll of the fleet is one write, not N).
    pub fn write_many(&self, samples: &[Sample]) -> Result<usize, LedgerError> {
        if samples.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        let mut written = 0_usize;
        {
            let mut insert = tx.prepare(
                "INSERT INTO measurements (pack_id, ts, power_w, soc_pct, kind)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(pack_id, ts) DO UPDATE SET
                    power_w = excluded.power_w,
                    soc_pct = excluded.soc_pct,
                    kind = excluded.kind",
            )?;
            for sample in samples {
                insert.execute(params![
                    sample.pack_id,
                    sample.ts,
                    sample.power_w,
                    sample.soc_pct,
                    sample.kind.as_str(),
                ])?;
                written += 1;
            }
        }
        tx.commit()?;
        Ok(written)
    }

    /// Rows of one tier for one pack in `[from, to)`, ascending.
    pub fn range(
        &self,
        tier: Tier,
        pack_id: i64,
        from: i64,
        to: i64,
    ) -> Result<Vec<Measurement>, LedgerError> {
        let sql = match tier {
            Tier::OneMinute => {
                "SELECT pack_id, ts, power_w, soc_pct, 1 FROM measurements
                 WHERE pack_id = ?1 AND ts >= ?2 AND ts < ?3 ORDER BY ts"
            }
            Tier::FiveMinute => {
                "SELECT pack_id, ts, power_w, soc_pct, samples FROM measurements_5m
                 WHERE pack_id = ?1 AND ts >= ?2 AND ts < ?3 ORDER BY ts"
            }
        };
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement.query_map(params![pack_id, from, to], Measurement::from_row)?;
        let mut measurements = Vec::new();
        for row in rows {
            measurements.push(row?);
        }
        Ok(measurements)
    }

    /// The newest 1-minute sample of a pack, if any.
    pub fn latest(&self, pack_id: i64) -> Result<Option<Measurement>, LedgerError> {
        let measurement = self
            .conn
            .query_row(
                "SELECT pack_id, ts, power_w, soc_pct, 1 FROM measurements
                 WHERE pack_id = ?1 ORDER BY ts DESC LIMIT 1",
                params![pack_id],
                Measurement::from_row,
            )
            .optional()?;
        Ok(measurement)
    }

    /// Recompute the 5-minute tier for `[from, to)` (aligned to 5-minute buckets).
    pub fn rollup(&self, from: i64, to: i64) -> Result<RollupReport, LedgerError> {
        rollup::run(&self.conn, from, to)
    }
}

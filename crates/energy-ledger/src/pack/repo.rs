//! Pack reads and the serial-keyed upsert.
//!
//! Discovery may re-register the same serial every poll; the upsert keeps `first_seen`, only moves
//! `last_seen` forward, and never blanks a known capacity or limit with a `NULL` from a later
//! report. The operator's `alias` is not part of the upsert at all: only [`Packs::set_alias`] writes
//! it, so a re-registration cannot rename a pack back. `manual_round_trip` is likewise written
//! only by [`Packs::set_manual_round_trip`]; discovery never overwrites the operator's entry.

use std::sync::MutexGuard;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::LedgerError;
use crate::measurement::Measurement;
use crate::pack::{Pack, PackField, PackState};

const COLUMNS: &str = "id, sn, name, alias, label_capacity_kwh, reported_capacity_kwh, max_charge_w, max_discharge_w, \
     first_seen, last_seen, learned_capacity_kwh, learned_round_trip, learned_confidence, learned_at, \
     last_full_charge_at, dwell_started_at, manual_round_trip";

/// Repository for the `packs` table, holding the ledger connection guard for the duration of a
/// scope.
pub struct Packs<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> Packs<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// Insert a pack by serial or update it, returning the stored row.
    pub fn upsert(&self, pack: &Pack) -> Result<Pack, LedgerError> {
        let sql = format!(
            "INSERT INTO packs
                (sn, name, label_capacity_kwh, reported_capacity_kwh, max_charge_w, max_discharge_w,
                 first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(sn) DO UPDATE SET
                name = COALESCE(excluded.name, packs.name),
                label_capacity_kwh = excluded.label_capacity_kwh,
                reported_capacity_kwh =
                    COALESCE(excluded.reported_capacity_kwh, packs.reported_capacity_kwh),
                max_charge_w = COALESCE(excluded.max_charge_w, packs.max_charge_w),
                max_discharge_w = COALESCE(excluded.max_discharge_w, packs.max_discharge_w),
                last_seen = MAX(packs.last_seen, excluded.last_seen)
             RETURNING {COLUMNS}"
        );
        let stored = self.conn.query_row(
            &sql,
            params![
                pack.sn,
                pack.name,
                pack.label_capacity_kwh,
                pack.reported_capacity_kwh,
                pack.max_charge_w,
                pack.max_discharge_w,
                pack.first_seen,
                pack.last_seen,
            ],
            Pack::from_row,
        )?;
        Ok(stored)
    }

    /// Publish one learning pass on the pack row (capacity, round trip, confidence and instant).
    pub fn set_learning(
        &self,
        id: i64,
        capacity_kwh: Option<f64>,
        round_trip: f64,
        confidence: f64,
        at: i64,
    ) -> Result<(), LedgerError> {
        self.conn.execute(
            "UPDATE packs SET learned_capacity_kwh = ?2, learned_round_trip = ?3,
                learned_confidence = ?4, learned_at = ?5
             WHERE id = ?1",
            params![id, capacity_kwh, round_trip, confidence, at],
        )?;
        Ok(())
    }

    /// Set or clear the operator's alias, returning the stored row (`None` for an unknown serial).
    ///
    /// The alias is the only pack field the API writes; discovery never passes one here, so a
    /// later upsert leaves it in place.
    pub fn set_alias(&self, sn: &str, alias: Option<&str>) -> Result<Option<Pack>, LedgerError> {
        let sql = format!("UPDATE packs SET alias = ?2 WHERE sn = ?1 RETURNING {COLUMNS}");
        let pack = self
            .conn
            .query_row(&sql, params![sn, alias], Pack::from_row)
            .optional()?;
        Ok(pack)
    }

    /// Set or clear the operator's pre-learning round trip, returning the stored row.
    ///
    /// The physical range is validated by the caller; this repository stores the fraction as
    /// given, so a corrupt row can be inspected even though readers ignore it.
    pub fn set_manual_round_trip(
        &self,
        sn: &str,
        round_trip: Option<f64>,
    ) -> Result<Option<Pack>, LedgerError> {
        let sql =
            format!("UPDATE packs SET manual_round_trip = ?2 WHERE sn = ?1 RETURNING {COLUMNS}");
        let pack = self
            .conn
            .query_row(&sql, params![sn, round_trip], Pack::from_row)
            .optional()?;
        Ok(pack)
    }

    /// Apply the operator's alias and manual round trip in one transaction.
    ///
    /// Both fields are optional edits: `Keep` leaves the stored value alone and `Clear` writes
    /// `NULL`. A failure in either statement rolls both back, so a rejected PATCH cannot leave a
    /// half-written pack.
    pub fn patch(
        &self,
        sn: &str,
        alias: PackField<&str>,
        manual: PackField<f64>,
    ) -> Result<Option<Pack>, LedgerError> {
        let tx = self.conn.unchecked_transaction()?;
        match alias {
            PackField::Set(value) => {
                tx.execute(
                    "UPDATE packs SET alias = ?2 WHERE sn = ?1",
                    params![sn, value],
                )?;
            }
            PackField::Clear => {
                tx.execute("UPDATE packs SET alias = NULL WHERE sn = ?1", params![sn])?;
            }
            PackField::Keep => {}
        }
        match manual {
            PackField::Set(value) => {
                tx.execute(
                    "UPDATE packs SET manual_round_trip = ?2 WHERE sn = ?1",
                    params![sn, value],
                )?;
            }
            PackField::Clear => {
                tx.execute(
                    "UPDATE packs SET manual_round_trip = NULL WHERE sn = ?1",
                    params![sn],
                )?;
            }
            PackField::Keep => {}
        }
        let pack = tx
            .query_row(
                &format!("SELECT {COLUMNS} FROM packs WHERE sn = ?1"),
                params![sn],
                Pack::from_row,
            )
            .optional()?;
        if pack.is_some() {
            tx.commit()?;
        }
        Ok(pack)
    }

    /// Record that a pack entered its balancing dwell. A repeated start never moves the stamp.
    pub fn begin_dwell(&self, sn: &str, at: i64) -> Result<(), LedgerError> {
        self.conn.execute(
            "UPDATE packs SET dwell_started_at = ?2
             WHERE sn = ?1 AND dwell_started_at IS NULL",
            params![sn, at],
        )?;
        Ok(())
    }

    /// Stamp a completed dwell: this is the only event that starts the seven-day clock.
    pub fn finish_dwell(&self, sn: &str, at: i64) -> Result<(), LedgerError> {
        self.conn.execute(
            "UPDATE packs SET last_full_charge_at = ?2, dwell_started_at = NULL WHERE sn = ?1",
            params![sn, at],
        )?;
        Ok(())
    }

    /// Abandon a dwell whose pack slipped off the target or whose run timed out.
    pub fn abort_dwell(&self, sn: &str) -> Result<(), LedgerError> {
        self.conn.execute(
            "UPDATE packs SET dwell_started_at = NULL WHERE sn = ?1",
            params![sn],
        )?;
        Ok(())
    }

    pub fn get(&self, id: i64) -> Result<Option<Pack>, LedgerError> {
        let sql = format!("SELECT {COLUMNS} FROM packs WHERE id = ?1");
        let pack = self
            .conn
            .query_row(&sql, params![id], Pack::from_row)
            .optional()?;
        Ok(pack)
    }

    pub fn by_sn(&self, sn: &str) -> Result<Option<Pack>, LedgerError> {
        let sql = format!("SELECT {COLUMNS} FROM packs WHERE sn = ?1");
        let pack = self
            .conn
            .query_row(&sql, params![sn], Pack::from_row)
            .optional()?;
        Ok(pack)
    }

    pub fn all(&self) -> Result<Vec<Pack>, LedgerError> {
        let sql = format!("SELECT {COLUMNS} FROM packs ORDER BY id");
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([], Pack::from_row)?;
        let mut packs = Vec::new();
        for row in rows {
            packs.push(row?);
        }
        Ok(packs)
    }

    /// Every pack with its newest 1-minute sample (one query per pack; the fleet has four).
    pub fn states(&self) -> Result<Vec<PackState>, LedgerError> {
        let mut states = Vec::new();
        for pack in self.all()? {
            let last = self
                .conn
                .query_row(
                    "SELECT pack_id, ts, power_w, soc_pct, 1 FROM measurements
                     WHERE pack_id = ?1 ORDER BY ts DESC LIMIT 1",
                    params![pack.id],
                    Measurement::from_row,
                )
                .optional()?;
            states.push(PackState { pack, last });
        }
        Ok(states)
    }
}

//! Pack reads and the serial-keyed upsert.
//!
//! Discovery may re-register the same serial every poll; the upsert keeps `first_seen`, only moves
//! `last_seen` forward, and never blanks a known capacity or limit with a `NULL` from a later
//! report.

use std::sync::MutexGuard;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::LedgerError;
use crate::measurement::Measurement;
use crate::pack::{Pack, PackState};

const COLUMNS: &str = "id, sn, name, label_capacity_kwh, reported_capacity_kwh, max_charge_w, max_discharge_w, \
     first_seen, last_seen, learned_capacity_kwh, learned_round_trip, learned_confidence, learned_at";

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

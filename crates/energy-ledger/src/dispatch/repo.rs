//! Dispatch-log persistence.

use std::sync::MutexGuard;

use rusqlite::{Connection, params};

use crate::dispatch::DispatchEntry;
use crate::error::LedgerError;

const COLUMNS: &str = "id, ts, pack_id, requested_w, applied_w, reason, mode, verified, error";

/// Repository for the `dispatch_log` table.
pub struct DispatchLog<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> DispatchLog<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// Append one dispatch attempt and return its id.
    pub fn insert(&self, entry: &DispatchEntry) -> Result<i64, LedgerError> {
        self.conn.execute(
            "INSERT INTO dispatch_log
                (ts, pack_id, requested_w, applied_w, reason, mode, verified, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                entry.ts,
                entry.pack_id,
                entry.requested_w,
                entry.applied_w,
                entry.reason,
                entry.mode,
                entry.verified,
                entry.error,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Dispatch rows in `[from, to)`, ascending.
    pub fn range(&self, from: i64, to: i64) -> Result<Vec<DispatchEntry>, LedgerError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM dispatch_log
             WHERE ts >= ?1 AND ts < ?2 ORDER BY ts, id"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![from, to], DispatchEntry::from_row)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }
}

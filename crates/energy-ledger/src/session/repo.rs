//! Session repository: re-runnable window extraction plus range reads.

use std::sync::MutexGuard;

use rusqlite::{Connection, params};

use crate::error::LedgerError;
use crate::measurement::Tier;
use crate::session::{ExtractReport, Session, SessionRules, extract};

/// Repository for the `sessions` table.
pub struct Sessions<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> Sessions<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// Re-extract one pack's sessions from `tier` samples in `[from, to)`.
    ///
    /// Existing sessions whose start lies in the window are deleted and rewritten in the same
    /// transaction, so running the same window twice produces the same rows.
    pub fn extract(
        &self,
        pack_id: i64,
        tier: Tier,
        from: i64,
        to: i64,
        rules: &SessionRules,
    ) -> Result<ExtractReport, LedgerError> {
        extract::run(&self.conn, pack_id, tier, from, to, rules)
    }

    /// Sessions of all packs that started in `[from, to)`, ascending.
    pub fn range(&self, from: i64, to: i64) -> Result<Vec<Session>, LedgerError> {
        self.query(
            "SELECT id, pack_id, direction, started_at, ended_at, soc_start, soc_end,
                    energy_ac_kwh, duration_s, sample_count, complete, invalidated_reason
             FROM sessions WHERE started_at >= ?1 AND started_at < ?2
             ORDER BY started_at, id",
            params![from, to],
        )
    }

    /// Sessions of one pack that started in `[from, to)`, ascending.
    pub fn for_pack(&self, pack_id: i64, from: i64, to: i64) -> Result<Vec<Session>, LedgerError> {
        self.query(
            "SELECT id, pack_id, direction, started_at, ended_at, soc_start, soc_end,
                    energy_ac_kwh, duration_s, sample_count, complete, invalidated_reason
             FROM sessions WHERE pack_id = ?1 AND started_at >= ?2 AND started_at < ?3
             ORDER BY started_at, id",
            params![pack_id, from, to],
        )
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<Session>, LedgerError> {
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement.query_map(params, Session::from_row)?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }
        Ok(sessions)
    }
}

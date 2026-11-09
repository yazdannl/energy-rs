//! Reads and writes the single pool row.

use std::sync::MutexGuard;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::LedgerError;
use crate::pool::PoolRow;

/// Repository for `energy_pool`.
pub struct Pool<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> Pool<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// The stored pool, `None` before the first advance.
    pub fn get(&self) -> Result<Option<PoolRow>, LedgerError> {
        let row = self
            .conn
            .query_row(
                "SELECT kwh, cost_eur, updated_at FROM energy_pool WHERE id = 1",
                [],
                |row| {
                    Ok(PoolRow {
                        kwh: row.get(0)?,
                        cost_eur: row.get(1)?,
                        updated_at: row.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Replace the pool.
    pub fn set(&self, pool: &PoolRow) -> Result<(), LedgerError> {
        self.conn.execute(
            "INSERT INTO energy_pool (id, kwh, cost_eur, updated_at) VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
                kwh = excluded.kwh,
                cost_eur = excluded.cost_eur,
                updated_at = excluded.updated_at",
            params![pool.kwh, pool.cost_eur, pool.updated_at],
        )?;
        Ok(())
    }
}

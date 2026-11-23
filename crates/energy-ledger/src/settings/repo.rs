//! Settings reads, writes and as-of reads.
//!
//! `settings` holds one current row per key. `settings_history` is append-only: every value change
//! adds a row at its `valid_from`, so a replay can ask what a key meant at a past instant. Writing
//! the same value again only moves `updated_at`; it does not add a history row.

use std::sync::MutexGuard;

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::error::LedgerError;
use crate::outbox::PendingLearningEvent;

/// Repository for `settings` and `settings_history`.
pub struct Settings<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> Settings<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// The current value of a key.
    pub fn get(&self, key: &str) -> Result<Option<String>, LedgerError> {
        let value = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value)
    }

    /// Set a key at `at`, appending history only when the value actually changes.
    pub fn set(&self, key: &str, value: &str, at: i64) -> Result<(), LedgerError> {
        if self.get(key)?.as_deref() == Some(value) {
            self.conn.execute(
                "UPDATE settings SET updated_at = MAX(updated_at, ?2) WHERE key = ?1",
                params![key, at],
            )?;
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        write_setting(&tx, key, value, at, true)?;
        tx.commit()?;
        Ok(())
    }

    /// Set a key and stage one learning audit row in the same transaction.
    ///
    /// Returns the outbox row id the caller must use when it later publishes the event.
    pub fn set_with_learning_event(
        &self,
        key: &str,
        value: &str,
        at: i64,
        event: &PendingLearningEvent,
    ) -> Result<i64, LedgerError> {
        let changed = self.get(key)?.as_deref() != Some(value);
        let tx = self.conn.unchecked_transaction()?;
        write_setting(&tx, key, value, at, changed)?;
        tx.execute(
            "INSERT INTO learning_outbox (pack_id, at, summary) VALUES (?1, ?2, ?3)",
            params![event.pack_id, event.at, event.summary],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    /// Remove a key; `true` when it existed. History is kept, so a removal is visible as-of.
    pub fn remove(&self, key: &str) -> Result<bool, LedgerError> {
        let existed = self.get(key)?.is_some();
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        Ok(existed)
    }

    /// The value in force at `at`, from the newest history row at or before it.
    pub fn as_of(&self, key: &str, at: i64) -> Result<Option<String>, LedgerError> {
        let value = self
            .conn
            .query_row(
                "SELECT value FROM settings_history
                 WHERE key = ?1 AND valid_from <= ?2
                 ORDER BY valid_from DESC, id DESC LIMIT 1",
                params![key, at],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value)
    }
}

fn write_setting(
    tx: &Transaction<'_>,
    key: &str,
    value: &str,
    at: i64,
    changed: bool,
) -> Result<(), LedgerError> {
    if !changed {
        tx.execute(
            "UPDATE settings SET updated_at = MAX(updated_at, ?2) WHERE key = ?1",
            params![key, at],
        )?;
        return Ok(());
    }
    tx.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET
            value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, at],
    )?;
    tx.execute(
        "INSERT INTO settings_history (key, value, valid_from) VALUES (?1, ?2, ?3)",
        params![key, value, at],
    )?;
    Ok(())
}

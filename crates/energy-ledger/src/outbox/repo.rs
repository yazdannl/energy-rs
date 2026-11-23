//! Reads and publishes pending learning audit events.

use std::sync::MutexGuard;

use rusqlite::{Connection, params};

use crate::error::LedgerError;
use crate::event::EventLevel;
use crate::outbox::PendingLearningEvent;

/// Repository for `learning_outbox`.
pub struct LearningOutbox<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> LearningOutbox<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// Pending rows, oldest first.
    pub fn pending(&self) -> Result<Vec<PendingLearningEvent>, LedgerError> {
        let mut statement = self
            .conn
            .prepare("SELECT id, pack_id, at, summary FROM learning_outbox ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            Ok(PendingLearningEvent::new(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
            ))
        })?;
        let mut pending = Vec::new();
        for row in rows {
            pending.push(row?);
        }
        Ok(pending)
    }

    /// Journal one event and clear its outbox row in the same transaction.
    ///
    /// The event kind and level are fixed: every outbox row is a `learning.applied` report. Writing
    /// and clearing together makes replay exactly-once even when two passes race across a restart.
    pub fn publish(&self, event: &PendingLearningEvent) -> Result<(), LedgerError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO events (ts, level, kind, message)
             VALUES (?1, ?2, 'learning.applied', ?3)",
            params![event.at, EventLevel::Info.as_str(), event.summary],
        )?;
        tx.execute(
            "DELETE FROM learning_outbox WHERE id = ?1",
            params![event.id],
        )?;
        tx.commit()?;
        Ok(())
    }
}

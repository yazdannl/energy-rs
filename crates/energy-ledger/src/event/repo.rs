//! Event persistence.

use std::sync::MutexGuard;

use rusqlite::{Connection, params};

use crate::error::LedgerError;
use crate::event::Event;

const COLUMNS: &str = "id, ts, level, kind, message, data_json";

/// Repository for the `events` table.
pub struct Events<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> Events<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// Append one event and return its id.
    pub fn insert(&self, event: &Event) -> Result<i64, LedgerError> {
        self.conn.execute(
            "INSERT INTO events (ts, level, kind, message, data_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.ts,
                event.level.as_str(),
                event.kind,
                event.message,
                event.data_json,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Events in `[from, to)`, ascending.
    pub fn range(&self, from: i64, to: i64) -> Result<Vec<Event>, LedgerError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM events
             WHERE ts >= ?1 AND ts < ?2 ORDER BY ts, id"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![from, to], Event::from_row)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// The newest `limit` events, newest first.
    pub fn recent(&self, limit: i64) -> Result<Vec<Event>, LedgerError> {
        let sql = format!("SELECT {COLUMNS} FROM events ORDER BY ts DESC, id DESC LIMIT ?1");
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![limit], Event::from_row)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }
}

//! Reads and publishes pending learning audit events.

use std::fmt::Write as _;
use std::sync::MutexGuard;

use chrono::Utc;
use rusqlite::types::{Type, ValueRef};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Row, Transaction, params};

use crate::error::LedgerError;
use crate::event::EventLevel;
use crate::outbox::{PendingLearningEvent, PublishOutcome, QuarantinedEvent};

/// Repository for `learning_outbox` and its quarantine table.
pub struct LearningOutbox<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> LearningOutbox<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// Pending rows, oldest first. Malformed rows are quarantined and skipped, not fatal.
    pub fn pending(&self) -> Result<Vec<PendingLearningEvent>, LedgerError> {
        let mut valid = Vec::new();
        let mut poison = Vec::new();
        {
            let mut statement = self
                .conn
                .prepare("SELECT id, pack_id, at, summary FROM learning_outbox ORDER BY id")?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let id = row.get(0)?;
                match decode(row) {
                    Ok(event) => valid.push(event),
                    Err(error) => poison.push(QuarantineInput::malformed(row, id, &error)),
                }
            }
        }
        if !poison.is_empty() {
            let tx = self.conn.unchecked_transaction()?;
            let now = Utc::now().timestamp();
            for row in &poison {
                quarantine(&tx, row, now)?;
            }
            tx.commit()?;
        }
        Ok(valid)
    }

    /// Quarantined payloads, oldest first, for inspection after a row-specific failure.
    pub fn quarantined(&self) -> Result<Vec<QuarantinedEvent>, LedgerError> {
        let mut statement = self.conn.prepare(
            "SELECT id, source_id, pack_id, at, summary, raw_pack_id, raw_at, raw_summary,
                    reason, quarantined_at
             FROM learning_outbox_quarantine ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(QuarantinedEvent {
                id: row.get(0)?,
                source_id: row.get(1)?,
                pack_id: row.get(2)?,
                at: row.get(3)?,
                summary: row.get(4)?,
                raw_pack_id: row.get(5)?,
                raw_at: row.get(6)?,
                raw_summary: row.get(7)?,
                reason: row.get(8)?,
                quarantined_at: row.get(9)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Claim and journal one pending row.
    ///
    /// `DELETE ... RETURNING` claims the row inside the same transaction that writes the event, so
    /// a second publisher holding the same id finds nothing and writes no duplicate. The payload
    /// comes from the claimed row, never from the caller's possibly stale copy.
    pub fn publish(&self, event: &PendingLearningEvent) -> Result<PublishOutcome, LedgerError> {
        let tx = self.conn.unchecked_transaction()?;
        let claimed = tx
            .query_row(
                "DELETE FROM learning_outbox WHERE id = ?1
                 RETURNING id, pack_id, at, summary",
                params![event.id],
                decode,
            )
            .optional()?;
        let Some(claimed) = claimed else {
            tx.commit()?;
            return Ok(PublishOutcome::AlreadyClaimed);
        };
        match tx.execute(
            "INSERT INTO events (ts, level, kind, message)
             VALUES (?1, ?2, 'learning.applied', ?3)",
            params![claimed.at, EventLevel::Info.as_str(), claimed.summary],
        ) {
            Ok(_) => {
                tx.commit()?;
                Ok(PublishOutcome::Published)
            }
            Err(error) if row_poison(&error) => {
                let bad = QuarantineInput::poisoned(&claimed, error.to_string());
                quarantine(&tx, &bad, Utc::now().timestamp())?;
                tx.commit()?;
                Ok(PublishOutcome::Quarantined)
            }
            Err(error) => Err(error.into()),
        }
    }
}

/// A payload that cannot be journalled, kept verbatim where possible.
struct QuarantineInput {
    source_id: i64,
    pack_id: String,
    at: i64,
    summary: String,
    raw_pack_id: String,
    raw_at: String,
    raw_summary: String,
    reason: String,
}

impl QuarantineInput {
    fn poisoned(event: &PendingLearningEvent, reason: String) -> Self {
        Self {
            source_id: event.id,
            pack_id: event.pack_id.clone(),
            at: event.at,
            summary: event.summary.clone(),
            raw_pack_id: event.pack_id.clone(),
            raw_at: event.at.to_string(),
            raw_summary: event.summary.clone(),
            reason,
        }
    }

    fn malformed(row: &Row<'_>, source_id: i64, error: &rusqlite::Error) -> Self {
        let pack_id = row.get_ref(1).map_or_else(|_| "null".to_owned(), raw);
        let at = row.get_ref(2).map_or_else(|_| "null".to_owned(), raw);
        let summary = row.get_ref(3).map_or_else(|_| "null".to_owned(), raw);
        Self {
            source_id,
            pack_id: pack_id.clone(),
            at: 0,
            summary: summary.clone(),
            raw_pack_id: pack_id,
            raw_at: at,
            raw_summary: summary,
            reason: error.to_string(),
        }
    }
}

fn decode(row: &Row<'_>) -> Result<PendingLearningEvent, rusqlite::Error> {
    let id = row.get(0)?;
    let pack_id = text(row.get_ref(1)?, "pack_id")?;
    let at = row.get(2)?;
    let summary = text(row.get_ref(3)?, "summary")?;
    Ok(PendingLearningEvent::new(id, pack_id, at, summary))
}

fn text(value: ValueRef<'_>, field: &str) -> Result<String, rusqlite::Error> {
    match value {
        ValueRef::Text(bytes) => String::from_utf8(bytes.to_vec()).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
        }),
        other => Err(rusqlite::Error::InvalidColumnType(
            0,
            field.to_owned(),
            other.data_type(),
        )),
    }
}

fn raw(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "null".to_owned(),
        ValueRef::Integer(value) => value.to_string(),
        ValueRef::Real(value) => value.to_string(),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        ValueRef::Blob(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                let _ = write!(&mut hex, "{byte:02x}");
            }
            format!("blob[{}]:{hex}", bytes.len())
        }
    }
}

fn row_poison(error: &rusqlite::Error) -> bool {
    match error {
        rusqlite::Error::SqliteFailure(ffi, _) => matches!(
            ffi.code,
            ErrorCode::ConstraintViolation | ErrorCode::TypeMismatch | ErrorCode::TooBig
        ),
        rusqlite::Error::IntegralValueOutOfRange(..) | rusqlite::Error::NulError(_) => true,
        _ => false,
    }
}

fn quarantine(tx: &Transaction<'_>, bad: &QuarantineInput, now: i64) -> Result<(), LedgerError> {
    tx.execute(
        "INSERT INTO learning_outbox_quarantine
            (source_id, pack_id, at, summary, raw_pack_id, raw_at, raw_summary, reason,
             quarantined_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            bad.source_id,
            bad.pack_id,
            bad.at,
            bad.summary,
            bad.raw_pack_id,
            bad.raw_at,
            bad.raw_summary,
            bad.reason,
            now
        ],
    )?;
    tx.execute(
        "DELETE FROM learning_outbox WHERE id = ?1",
        params![bad.source_id],
    )?;
    tx.execute(
        "INSERT INTO events (ts, level, kind, message)
         VALUES (?1, ?2, 'learning.outbox_quarantined', ?3)",
        params![
            now,
            EventLevel::Error.as_str(),
            format!(
                "outbox row {} for {}: {}",
                bad.source_id, bad.pack_id, bad.reason
            )
        ],
    )?;
    Ok(())
}

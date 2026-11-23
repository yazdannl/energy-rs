//! Reads and publishes pending learning audit events.

use std::sync::MutexGuard;

use chrono::Utc;
use rusqlite::types::{Type, ValueRef};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Row, Transaction, params};

use crate::error::LedgerError;
use crate::event::EventLevel;
use crate::outbox::{PendingLearningEvent, PublishOutcome, QuarantinedEvent, RawValue};

/// SQLite type tag followed by lossless bytes for every outbox payload field.
///
/// REALs use `printf('%!.17g')`, which round-trips IEEE-754 doubles; `CAST(real AS BLOB)` would
/// only keep SQLite's shorter text rendering.
const RAW_COLUMNS: &str = "typeof(pack_id),
    CASE WHEN typeof(pack_id) = 'real' THEN CAST(printf('%!.17g', pack_id) AS BLOB)
         ELSE CAST(pack_id AS BLOB) END,
    typeof(at),
    CASE WHEN typeof(at) = 'real' THEN CAST(printf('%!.17g', at) AS BLOB)
         ELSE CAST(at AS BLOB) END,
    typeof(summary),
    CASE WHEN typeof(summary) = 'real' THEN CAST(printf('%!.17g', summary) AS BLOB)
         ELSE CAST(summary AS BLOB) END";

/// Repository for `learning_outbox` and its quarantine table.
pub struct LearningOutbox<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> LearningOutbox<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// Pending rows, oldest first. Malformed rows are claimed and quarantined, not fatal.
    pub fn pending(&self) -> Result<Vec<PendingLearningEvent>, LedgerError> {
        let mut valid = Vec::new();
        let mut malformed = Vec::new();
        {
            let mut statement = self
                .conn
                .prepare("SELECT id, pack_id, at, summary FROM learning_outbox ORDER BY id")?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let id = row.get(0)?;
                match decode(row) {
                    Ok(event) => valid.push(event),
                    Err(_) => malformed.push(id),
                }
            }
        }
        for id in malformed {
            self.settle_malformed(id)?;
        }
        Ok(valid)
    }

    /// Quarantined payloads, oldest first, with original SQLite types and bytes.
    pub fn quarantined(&self) -> Result<Vec<QuarantinedEvent>, LedgerError> {
        let mut statement = self.conn.prepare(
            "SELECT id, source_id,
                    pack_id_type, pack_id_blob, at_type, at_blob, summary_type, summary_blob,
                    reason, quarantined_at
             FROM learning_outbox_quarantine ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(QuarantinedEvent {
                id: row.get(0)?,
                source_id: row.get(1)?,
                pack_id: raw_value(row, 2)?,
                at: raw_value(row, 4)?,
                summary: raw_value(row, 6)?,
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
        let Some(claimed) = claim(&tx, event.id)? else {
            tx.commit()?;
            return Ok(PublishOutcome::AlreadyClaimed);
        };
        let outcome = settle(&tx, &claimed, Utc::now().timestamp())?;
        tx.commit()?;
        Ok(outcome)
    }

    /// Re-read and claim one row that failed the first decode. A prior claim is a no-op.
    fn settle_malformed(&self, id: i64) -> Result<(), LedgerError> {
        let tx = self.conn.unchecked_transaction()?;
        if let Some(claimed) = claim(&tx, id)? {
            let _ = settle(&tx, &claimed, Utc::now().timestamp())?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// One claimed row, preserved with SQLite storage types and original bytes.
struct Claimed {
    id: i64,
    pack_id: RawValue,
    at: RawValue,
    summary: RawValue,
}

impl Claimed {
    fn decode(&self) -> Result<PendingLearningEvent, rusqlite::Error> {
        Ok(PendingLearningEvent::new(
            self.id,
            decode_text(&self.pack_id, "pack_id")?,
            decode_integer(&self.at, "at")?,
            decode_text(&self.summary, "summary")?,
        ))
    }
}

fn claim(tx: &Transaction<'_>, id: i64) -> Result<Option<Claimed>, LedgerError> {
    let sql = format!("DELETE FROM learning_outbox WHERE id = ?1 RETURNING id, {RAW_COLUMNS}");
    tx.query_row(&sql, params![id], |row| {
        Ok(Claimed {
            id: row.get(0)?,
            pack_id: raw_value(row, 1)?,
            at: raw_value(row, 3)?,
            summary: raw_value(row, 5)?,
        })
    })
    .optional()
    .map_err(Into::into)
}

fn settle(
    tx: &Transaction<'_>,
    claimed: &Claimed,
    now: i64,
) -> Result<PublishOutcome, LedgerError> {
    match claimed.decode() {
        Ok(event) => match insert_event(tx, &event) {
            Ok(_) => Ok(PublishOutcome::Published),
            Err(error) if row_poison(&error) => {
                let reason = error.to_string();
                quarantine(tx, claimed, &reason, now)?;
                Ok(PublishOutcome::Quarantined)
            }
            Err(error) => Err(error.into()),
        },
        Err(error) => {
            let reason = error.to_string();
            quarantine(tx, claimed, &reason, now)?;
            Ok(PublishOutcome::Quarantined)
        }
    }
}

fn insert_event(
    tx: &Transaction<'_>,
    event: &PendingLearningEvent,
) -> Result<usize, rusqlite::Error> {
    tx.execute(
        "INSERT INTO events (ts, level, kind, message)
         VALUES (?1, ?2, 'learning.applied', ?3)",
        params![event.at, EventLevel::Info.as_str(), event.summary],
    )
}

fn quarantine(
    tx: &Transaction<'_>,
    claimed: &Claimed,
    reason: &str,
    now: i64,
) -> Result<(), LedgerError> {
    let inserted = tx.execute(
        "INSERT INTO learning_outbox_quarantine
            (source_id, pack_id_type, pack_id_blob, at_type, at_blob,
             summary_type, summary_blob, reason, quarantined_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(source_id) DO NOTHING",
        params![
            claimed.id,
            claimed.pack_id.storage_type,
            claimed.pack_id.bytes,
            claimed.at.storage_type,
            claimed.at.bytes,
            claimed.summary.storage_type,
            claimed.summary.bytes,
            reason,
            now
        ],
    )?;
    if inserted > 0 {
        tx.execute(
            "INSERT INTO events (ts, level, kind, message)
             VALUES (?1, ?2, 'learning.outbox_quarantined', ?3)",
            params![
                now,
                EventLevel::Error.as_str(),
                format!(
                    "outbox row {} for {}: {}",
                    claimed.id,
                    claimed.pack_id.label(),
                    reason
                )
            ],
        )?;
    }
    Ok(())
}

fn raw_value(row: &Row<'_>, index: usize) -> Result<RawValue, rusqlite::Error> {
    let storage_type: String = row.get(index)?;
    let bytes: Option<Vec<u8>> = row.get(index + 1)?;
    Ok(RawValue::new(storage_type, bytes))
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
        other => Err(type_error(field, other.data_type())),
    }
}

fn decode_text(value: &RawValue, field: &str) -> Result<String, rusqlite::Error> {
    match value.storage_type.as_str() {
        "text" => String::from_utf8(value.bytes.clone()).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
        }),
        "integer" => Err(type_error(field, Type::Integer)),
        "real" => Err(type_error(field, Type::Real)),
        "blob" => Err(type_error(field, Type::Blob)),
        "null" => Err(type_error(field, Type::Null)),
        other => Err(rusqlite::Error::InvalidParameterName(other.to_owned())),
    }
}

fn decode_integer(value: &RawValue, field: &str) -> Result<i64, rusqlite::Error> {
    if value.storage_type != "integer" {
        return Err(type_error(field, storage_type(&value.storage_type)));
    }
    let text = String::from_utf8(value.bytes.clone()).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
    })?;
    text.parse().map_err(|_| type_error(field, Type::Integer))
}

fn storage_type(tag: &str) -> Type {
    match tag {
        "integer" => Type::Integer,
        "real" => Type::Real,
        "text" => Type::Text,
        "blob" => Type::Blob,
        _ => Type::Null,
    }
}

fn type_error(field: &str, kind: Type) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(0, field.to_owned(), kind)
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

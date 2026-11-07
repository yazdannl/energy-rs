//! One journal entry.

use crate::event::EventLevel;

/// A journal row. `kind` is a stable machine key (`dispatch.hold`, `ha.offline`, ...), `message` is
/// for humans and `data_json` is an opaque payload the ledger never parses.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: i64,
    pub ts: i64,
    pub level: EventLevel,
    pub kind: String,
    pub message: String,
    pub data_json: Option<String>,
}

impl Event {
    pub fn new(
        ts: i64,
        level: EventLevel,
        kind: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: 0,
            ts,
            level,
            kind: kind.into(),
            message: message.into(),
            data_json: None,
        }
    }

    #[must_use]
    pub fn with_data(mut self, data_json: impl Into<String>) -> Self {
        self.data_json = Some(data_json.into());
        self
    }

    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let level_key: String = row.get(2)?;
        let level = EventLevel::from_key(&level_key).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(err))
        })?;
        Ok(Self {
            id: row.get(0)?,
            ts: row.get(1)?,
            level,
            kind: row.get(3)?,
            message: row.get(4)?,
            data_json: row.get(5)?,
        })
    }
}

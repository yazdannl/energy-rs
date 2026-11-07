//! Row counts and freshness of one ledger file, read without writing anything.
//!
//! The doctor opens a stopped or running ledger with `query_only` and reports what is in it: the
//! schema version, the file size, per-table row counts and the newest row of each table a caller can
//! reason about. Nothing here migrates or trims: a diagnostic must not be the reason a ledger
//! changes shape.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};

use crate::db::Ledger;
use crate::error::LedgerError;

/// What one ledger file contains, as of the read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LedgerStats {
    /// `meta.schema_version` (`0` when the file was never migrated).
    pub schema_version: i64,
    /// Size of the database file, bytes.
    pub size_bytes: u64,
    pub packs: i64,
    pub measurements_1m: i64,
    pub measurements_5m: i64,
    pub sessions: i64,
    pub plans: i64,
    pub plan_slots: i64,
    pub dispatch_log: i64,
    pub events: i64,
    pub savings_daily: i64,
    pub settings: i64,
    /// Newest 1-minute sample, UTC epoch seconds.
    pub last_measurement_at: Option<i64>,
    /// Newest stored plan, UTC epoch seconds.
    pub last_plan_at: Option<i64>,
    /// Newest dispatch row, UTC epoch seconds.
    pub last_dispatch_at: Option<i64>,
    /// Newest journal event, UTC epoch seconds.
    pub last_event_at: Option<i64>,
}

impl LedgerStats {
    /// Read the stats of an existing ledger file (no creation, no migration, no writes).
    pub fn read(path: impl AsRef<Path>) -> Result<Self, LedgerError> {
        let path = path.as_ref();
        let ledger = Ledger::open_read_only(path)?;
        ledger.stats()
    }

    /// Read stats over an already-open read-only ledger.
    pub(crate) fn read_connection(conn: &Connection) -> Result<Self, LedgerError> {
        conn.busy_timeout(Duration::from_millis(5_000))?;
        let counts = conn.query_row(
            "SELECT
                (SELECT COUNT(*) FROM packs),
                (SELECT COUNT(*) FROM measurements),
                (SELECT COUNT(*) FROM measurements_5m),
                (SELECT COUNT(*) FROM sessions),
                (SELECT COUNT(*) FROM plans),
                (SELECT COUNT(*) FROM plan_slots),
                (SELECT COUNT(*) FROM dispatch_log),
                (SELECT COUNT(*) FROM events),
                (SELECT COUNT(*) FROM savings_daily),
                (SELECT COUNT(*) FROM settings),
                (SELECT MAX(ts) FROM measurements),
                (SELECT MAX(created_at) FROM plans),
                (SELECT MAX(ts) FROM dispatch_log),
                (SELECT MAX(ts) FROM events)",
            [],
            |row| {
                Ok(Self {
                    schema_version: 0,
                    size_bytes: 0,
                    packs: row.get(0)?,
                    measurements_1m: row.get(1)?,
                    measurements_5m: row.get(2)?,
                    sessions: row.get(3)?,
                    plans: row.get(4)?,
                    plan_slots: row.get(5)?,
                    dispatch_log: row.get(6)?,
                    events: row.get(7)?,
                    savings_daily: row.get(8)?,
                    settings: row.get(9)?,
                    last_measurement_at: row.get(10)?,
                    last_plan_at: row.get(11)?,
                    last_dispatch_at: row.get(12)?,
                    last_event_at: row.get(13)?,
                })
            },
        )?;
        let version: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(Self {
            schema_version: version
                .and_then(|text| text.trim().parse::<i64>().ok())
                .unwrap_or(0),
            ..counts
        })
    }

    /// Whether the file holds any row at all (a freshly created ledger does not).
    pub fn is_empty(&self) -> bool {
        self.packs == 0
            && self.measurements_1m == 0
            && self.plans == 0
            && self.events == 0
            && self.settings == 0
    }
}

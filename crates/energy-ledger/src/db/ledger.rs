//! The ledger handle: one connection, pragmas, migrations, repository accessors and the retention
//! job.
//!
//! Every repository borrows the connection guard, so a caller gets serialized access without
//! seeing `rusqlite` and without a second writer racing the engine.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chrono::Utc;
use rusqlite::Connection;

use crate::db::config::LedgerConfig;
use crate::db::migrate;
use crate::dispatch::DispatchLog;
use crate::error::LedgerError;
use crate::event::Events;
use crate::measurement::Measurements;
use crate::pack::Packs;
use crate::plan::Plans;
use crate::retention::{RetentionPolicy, RetentionReport};
use crate::savings::Savings;
use crate::session::Sessions;
use crate::settings::Settings;
use crate::stats::LedgerStats;

/// The ledger database: a single WAL connection under a mutex.
///
/// Open once per process and pass `&Ledger` around; each accessor locks for the duration of the
/// returned repository scope. Opening the same file from a second process/connection is supported
/// (WAL + busy timeout), but the running system has exactly one writer.
pub struct Ledger {
    conn: Mutex<Connection>,
    path: PathBuf,
    retention: RetentionPolicy,
}

impl Ledger {
    /// Open (creating the file if needed) and migrate to the schema this binary supports.
    pub fn open(config: &LedgerConfig) -> Result<Self, LedgerError> {
        let mut conn = Connection::open(&config.path).map_err(|err| LedgerError::Open {
            path: config.path.display().to_string(),
            reason: err.to_string(),
        })?;
        conn.busy_timeout(Duration::from_millis(config.busy_timeout_ms))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        migrate::run(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: config.path.clone(),
            retention: config.retention,
        })
    }

    /// A throwaway in-memory ledger (tests, replays).
    pub fn open_in_memory() -> Result<Self, LedgerError> {
        Self::open(&LedgerConfig::at(":memory:"))
    }

    /// A ledger at `path` with the default timeout and retention.
    pub fn open_at(path: impl AsRef<Path>) -> Result<Self, LedgerError> {
        Self::open(&LedgerConfig::at(path.as_ref().to_path_buf()))
    }

    /// Open an existing ledger for reading only: no file creation, no migration, no writes.
    ///
    /// The connection is put in `query_only` mode, so a diagnostic command cannot migrate, trim or
    /// register a pack. A file that is missing or carries a newer schema is refused.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self, LedgerError> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(LedgerError::Open {
                path: path.display().to_string(),
                reason: "file does not exist".to_owned(),
            });
        }
        let conn = Connection::open(&path).map_err(|err| LedgerError::Open {
            path: path.display().to_string(),
            reason: err.to_string(),
        })?;
        conn.busy_timeout(Duration::from_millis(5_000))?;
        conn.pragma_update(None, "query_only", true)?;
        migrate::verify(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path,
            retention: RetentionPolicy::default(),
        })
    }

    /// Row counts and freshness, read through this handle without writing.
    pub fn stats(&self) -> Result<LedgerStats, LedgerError> {
        let conn = self.lock()?;
        let mut stats = LedgerStats::read_connection(&conn)?;
        stats.size_bytes = std::fs::metadata(&self.path).map_or(0, |meta| meta.len());
        Ok(stats)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn retention(&self) -> RetentionPolicy {
        self.retention
    }

    pub fn packs(&self) -> Result<Packs<'_>, LedgerError> {
        Ok(Packs::new(self.lock()?))
    }

    pub fn measurements(&self) -> Result<Measurements<'_>, LedgerError> {
        Ok(Measurements::new(self.lock()?))
    }

    pub fn sessions(&self) -> Result<Sessions<'_>, LedgerError> {
        Ok(Sessions::new(self.lock()?))
    }

    pub fn plans(&self) -> Result<Plans<'_>, LedgerError> {
        Ok(Plans::new(self.lock()?))
    }

    pub fn dispatch(&self) -> Result<DispatchLog<'_>, LedgerError> {
        Ok(DispatchLog::new(self.lock()?))
    }

    pub fn events(&self) -> Result<Events<'_>, LedgerError> {
        Ok(Events::new(self.lock()?))
    }

    pub fn savings(&self) -> Result<Savings<'_>, LedgerError> {
        Ok(Savings::new(self.lock()?))
    }

    /// The stored-energy cost pool.
    pub fn pool(&self) -> Result<crate::pool::Pool<'_>, LedgerError> {
        Ok(crate::pool::Pool::new(self.lock()?))
    }

    /// The latched pack selection.
    pub fn selection(&self) -> Result<crate::selection::Selection<'_>, LedgerError> {
        Ok(crate::selection::Selection::new(self.lock()?))
    }

    pub fn settings(&self) -> Result<Settings<'_>, LedgerError> {
        Ok(Settings::new(self.lock()?))
    }

    /// Run the retention job as seen at `now` (UTC epoch seconds).
    pub fn run_retention(&self, now: i64) -> Result<RetentionReport, LedgerError> {
        let conn = self.lock()?;
        crate::retention::run(&conn, self.retention, now)
    }

    /// Checkpoint the write-ahead log into the database file.
    ///
    /// Called on the graceful shutdown path so the add-on's cold backup sees a complete file instead
    /// of a hot WAL. A checkpoint is a no-op for an in-memory database.
    pub fn checkpoint(&self) -> Result<(), LedgerError> {
        let conn = self.lock()?;
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_row| Ok(()))?;
        Ok(())
    }

    /// Run the retention job as of this instant.
    pub fn run_retention_now(&self) -> Result<RetentionReport, LedgerError> {
        self.run_retention(Utc::now().timestamp())
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, LedgerError> {
        self.conn.lock().map_err(|_| LedgerError::Poisoned)
    }
}

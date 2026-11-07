//! Ledger location, SQLite busy timeout and the retention policy used by the trim job.

use std::path::PathBuf;

use crate::retention::RetentionPolicy;

/// Where the ledger lives and how patient it is with a locked file.
///
/// The default path is the add-on data volume (`/data/energy_rs.db`), the timeout is 5 s (the
/// dispatcher may write while the UI reads) and the retention policy is the PLAN.md one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerConfig {
    pub path: PathBuf,
    pub busy_timeout_ms: u64,
    pub retention: RetentionPolicy,
}

impl Default for LedgerConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("/data/energy_rs.db"),
            busy_timeout_ms: 5_000,
            retention: RetentionPolicy::default(),
        }
    }
}

impl LedgerConfig {
    /// Configuration for a specific file (use `:memory:` for a throwaway in-memory database).
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_busy_timeout(mut self, busy_timeout_ms: u64) -> Self {
        self.busy_timeout_ms = busy_timeout_ms;
        self
    }

    #[must_use]
    pub fn with_retention(mut self, retention: RetentionPolicy) -> Self {
        self.retention = retention;
        self
    }
}

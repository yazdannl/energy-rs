//! The one error type the ledger returns.

use thiserror::Error;

/// Everything the ledger can refuse.
///
/// A database that carries a schema version newer than this binary is an error and not a
/// best-effort open: running an old binary against a migrated file is how a ledger loses rows.
#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("ledger at {path} could not be opened: {reason}")]
    Open { path: String, reason: String },
    #[error("ledger schema version {found} is newer than supported version {supported}")]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("ledger migration {version} failed: {reason}")]
    Migrate { version: i64, reason: String },
    #[error("ledger connection is poisoned")]
    Poisoned,
    #[error("ledger row is not usable: {0}")]
    Invalid(String),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

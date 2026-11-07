//! Read-only inspection: schema version, table sizes and freshness for the doctor.

mod ledger_stats;

pub use ledger_stats::LedgerStats;

/// The schema version this binary writes and can read.
pub const SUPPORTED_SCHEMA_VERSION: i64 = crate::db::migrate::SCHEMA_VERSION;

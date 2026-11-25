//! Audited manual release of a fail-closed import guard.

/// One explicit manual release, retained for the M4 row and the ledger.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReleaseRecord {
    pub pack_id: String,
    pub actor: String,
    pub reason: String,
    pub at_ms: i64,
}

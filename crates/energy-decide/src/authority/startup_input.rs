//! Exactly the inputs needed by the universal startup resolution.

use crate::authority::{AnchorStore, AuthorityLedger, ResolutionStore};

/// Inputs to startup resolution; the ledger is supplied by the external store.
pub struct StartupInput<'a> {
    /// Ledger exactly as found on disk.
    pub ledger: AuthorityLedger,
    /// Separate-file anchor store.
    pub anchors: &'a dyn AnchorStore,
    /// Append-only startup-resolution record store.
    pub resolutions: &'a dyn ResolutionStore,
    /// Startup instant, epoch milliseconds.
    pub now_ms: i64,
}

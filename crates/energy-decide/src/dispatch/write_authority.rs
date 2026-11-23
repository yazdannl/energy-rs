//! Provisional M6 consumer port; M4 never decides authority itself.

use crate::dispatch::{AuthorityDecision, PackCommand};

/// Mirrors the M6 write-authority gate at execution time.
pub trait WriteAuthority: Send + Sync + 'static {
    /// Authorize exactly this command at execution time.
    fn authorize(&self, command: &PackCommand) -> AuthorityDecision;
}

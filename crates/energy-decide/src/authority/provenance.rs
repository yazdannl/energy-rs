//! Three-condition legacy provenance test and post-token recovery routing.

use crate::authority::{AuthorityLedger, BootstrapReason, WRITE_AUTHORITY_SCHEMA};

/// Outcome of examining a ledger before any authority decision is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Provenance {
    /// Empty install: audited generation-1 auto is allowed.
    FreshInstall,
    /// Proven legacy ledger: audited migration is allowed with this reason.
    Legacy(BootstrapReason),
    /// Ambiguous, restored or post-token: operator recovery only.
    Recovery,
}

pub(crate) fn decide(ledger: &AuthorityLedger) -> Provenance {
    if ledger.latch().is_none() && ledger.events().is_empty() && ledger.persisted_mode().is_none() {
        return Provenance::FreshInstall;
    }
    if ledger.latch().is_some()
        || ledger.schema_version() >= WRITE_AUTHORITY_SCHEMA
        || ledger.has_latch_history()
        || !ledger.journal_complete()
    {
        return Provenance::Recovery;
    }
    let reason = match ledger.persisted_mode() {
        Some("auto") => BootstrapReason::LegacyAutoMigration,
        Some("dry_run") => BootstrapReason::LegacyDryRunPreserved,
        Some("advisory") => BootstrapReason::LegacyAdvisoryMigratedToAuto,
        _ => BootstrapReason::UnknownModeMigratedToAuto,
    };
    Provenance::Legacy(reason)
}

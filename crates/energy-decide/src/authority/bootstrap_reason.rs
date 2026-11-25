//! Closed set of reasons for a sealed initialization or recovery event.

/// Reason carried by a narrowly typed initialization/recovery event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapReason {
    /// No ledger and no token.
    FreshInstall,
    /// Legacy schema and journal prove a prior auto mode.
    LegacyAutoMigration,
    /// Legacy persisted dry run must stay write-free.
    LegacyDryRunPreserved,
    /// Advisory was compute-only; migration to auto must be loud.
    LegacyAdvisoryMigratedToAuto,
    /// Unknown or missing legacy mode, named exactly in the audit event.
    UnknownModeMigratedToAuto,
    /// Logged operator recovery on a post-token ledger.
    OperatorRecovery,
}

impl BootstrapReason {
    /// Stable audit-event reason slug.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FreshInstall => "fresh_install",
            Self::LegacyAutoMigration => "legacy_auto_migration",
            Self::LegacyDryRunPreserved => "legacy_dry_run_preserved",
            Self::LegacyAdvisoryMigratedToAuto => "legacy_advisory_migrated_to_auto",
            Self::UnknownModeMigratedToAuto => "unknown_mode_migrated_to_auto",
            Self::OperatorRecovery => "operator_recovery",
        }
    }
}

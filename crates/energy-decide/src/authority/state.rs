//! The three M5 write-authority states consumed, never redefined, by M6.

/// Operational write-authority state. There is no `continuity_proven` and no fourth
/// operational state: `authority_recovery_required` is a health presentation of
/// `crate::authority::AuthorityPhase::Invalid` with the effective behaviour of
/// `DryRunLatched` and zero writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityState {
    /// Absolutely write-free; safety and holds included.
    DryRunLatched,
    /// Logged, bounded operator clearance; fenced safety-lane neutralisation only.
    Clearing,
    /// Economic and safety writes are allowed subject to normal operational rules.
    Auto,
}

impl AuthorityState {
    /// Stable name used in audit rows and API payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DryRunLatched => "dry_run_latched",
            Self::Clearing => "clearing",
            Self::Auto => "auto",
        }
    }
}

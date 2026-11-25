//! Result of the one universal startup resolution.

use crate::authority::{AuthorityPhase, AuthorityState, HealthReport, StartupResolution};

/// Exactly one startup resolution plus the health payload and gate eligibility.
#[derive(Debug, Clone)]
pub struct StartupOutcome {
    /// `reconciled` or `recovery_required`, never a continuity claim.
    pub resolution: StartupResolution,
    /// Effective operational state; recovery behaves write-free.
    pub state: AuthorityState,
    /// Effective authority phase.
    pub phase: AuthorityPhase,
    /// Typed reason for recovery, when recovery was selected.
    pub reason: Option<&'static str>,
    /// Id of the one record this startup appended.
    pub record_id: Option<u64>,
    /// True only when authority is initialized and reconciliation succeeded.
    pub gate_open: bool,
    /// True only with valid `auto` authority and a reconciled startup.
    pub dispatch_eligible: bool,
    /// Audited bootstrap warning, for example an advisory migration.
    pub warning: String,
    /// Dashboard and API health payload.
    pub health: HealthReport,
}

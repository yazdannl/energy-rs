//! Health payload shared verbatim by the dashboard and the API.

use crate::authority::RecoveryEvidence;

/// Visible protection status; text is identical in dashboard and API health.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthReport {
    /// Typed protection/battery codes joined by commas in the API payload.
    pub codes: Vec<&'static str>,
    /// Exact prominent human-readable text.
    pub text: &'static str,
    /// Universal continuity warning plus the failing reconciliation evidence when locked.
    pub banner: String,
    /// Distinct condition: `dry_run_latched`, `clearing`, `auto` or `recovery_required`.
    pub condition: &'static str,
    /// Structured evidence; present only for `recovery_required`.
    pub recovery: Option<RecoveryEvidence>,
    /// True only in auto including protection_active.
    pub safety_active: bool,
    /// True in latched, clearing and recovery.
    pub battery_unmanaged: bool,
}

impl HealthReport {
    /// API health payload string, preserving the frozen code order.
    pub fn code_string(&self) -> String {
        self.codes.join(",")
    }

    /// One-line detail shown with the recovery banner.
    pub fn recovery_detail(&self) -> Option<String> {
        self.recovery.as_ref().map(RecoveryEvidence::detail)
    }
}

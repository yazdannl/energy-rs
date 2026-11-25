//! The only two startup resolutions this platform can report.

/// Universal startup resolution. Continuity is never claimed as proven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StartupResolution {
    /// Anchor, ledger and resolution record reconcile; dispatch still needs valid `auto`.
    Reconciled,
    /// Anchor missing/mismatched, journal incomplete or authority invalid; no dispatch.
    RecoveryRequired,
}

impl StartupResolution {
    /// Stable name used in the startup-resolution record and API payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reconciled => "reconciled",
            Self::RecoveryRequired => "recovery_required",
        }
    }
}

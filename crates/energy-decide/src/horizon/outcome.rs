//! Successful M1 outcomes; refusals are a separate typed value.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizonOutcome {
    /// All requested slots are present and valid.
    Healthy,
    /// Provisional state while the mandatory refusal checks have not run.
    TailAccepted,
    /// Missing tail after all refusal checks passed; shortfall stays visible.
    DegradedAccepted,
}

impl HorizonOutcome {
    /// Whether the horizon alone permits economic dispatch.
    pub const fn economic_dispatch(self) -> bool {
        matches!(self, Self::Healthy | Self::DegradedAccepted)
    }
}

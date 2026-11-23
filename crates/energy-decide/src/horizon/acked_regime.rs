//! Regime named in an operator acknowledgement, bound to the stored configuration.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcknowledgedRegime {
    /// Operator acknowledged forced netting on.
    On,
    /// Operator acknowledged forced netting off.
    Off,
}

impl AcknowledgedRegime {
    /// Stable external spelling of the acknowledged regime.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
        }
    }
}

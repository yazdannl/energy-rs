//! Which export rule the plan uses (`docs/POLICY.md` §3 R1).

/// Which export rule the plan uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportPolicy {
    /// Pre-2027 behaviour: the DP never plans a battery export.
    #[default]
    Legacy,
    /// Post-net-metering: export only above the break-even of the stored kWh.
    CostBasis,
}

impl ExportPolicy {
    /// Parse a settings value; anything unknown is the safe default.
    pub fn parse(value: &str) -> Self {
        match value.trim() {
            "cost_basis" => Self::CostBasis,
            _ => Self::Legacy,
        }
    }

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::CostBasis => "cost_basis",
        }
    }
}

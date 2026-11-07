//! How the objective prices PV energy charged into the pack.

/// How the objective prices PV energy charged into the pack (`pv_charge_preference`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PvChargePreference {
    /// The slot's sell price: the export the charge forgoes.
    #[default]
    Auto,
    /// Zero: PV into the pack is free.
    PreferPv,
    /// The slot's buy price: PV into the pack is valued at what the house would have paid.
    AvoidPv,
}

impl PvChargePreference {
    pub(crate) fn code(self) -> i32 {
        match self {
            PvChargePreference::Auto => 0,
            PvChargePreference::PreferPv => 1,
            PvChargePreference::AvoidPv => 2,
        }
    }
}

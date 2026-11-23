//! The six canonical keys that make a tie deterministic.

/// Quantised canonical signature of the selected path.
#[derive(Debug, Clone, PartialEq)]
pub struct TieBreak {
    /// `round(objective_eur, 6)`.
    pub objective_rounded_eur: f64,
    /// First discharging slot index, or `N` when there is no discharge.
    pub first_discharge_slot_index: usize,
    /// Exact `Σ p_i²` in W², with no slot-duration factor.
    pub sum_power_squared: f64,
    /// Number of slots with nonzero battery power.
    pub nonzero_slot_count: usize,
    /// `[Q(e_1)..Q(e_N)]` in kWh at 1 Wh resolution.
    pub energy_signature_wh: Vec<i32>,
    /// `[Q(p_0)..Q(p_{N-1})]` in W at 1 W resolution.
    pub power_signature_w: Vec<i32>,
    /// `canonical_signatures` when all six keys coincide with another path.
    pub tie_resolved_by: &'static str,
}

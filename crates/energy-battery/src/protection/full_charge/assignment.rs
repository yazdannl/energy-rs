//! One pack's maintenance charge assignment.

use crate::protection::full_charge::charge_slot::ChargeSlot;

/// A due pack's charge, chosen slots and balancing window.
#[derive(Debug, Clone, PartialEq)]
pub struct FullChargeAssignment {
    /// Pack this assignment belongs to.
    pub pack_id: String,
    /// `interval_due` (seven-day rule) or `manual_request` (protection endpoint).
    pub reason: &'static str,
    /// Target state of charge, always 100 %.
    pub target_soc_pct: f64,
    /// The cheapest slots that cover the pack's missing energy.
    pub slots: Vec<ChargeSlot>,
    /// End of the last charge slot; the dwell starts after this.
    pub charge_end: i64,
    /// Earliest instant the balancing dwell can complete.
    pub dwell_until: i64,
    /// Expected import cost of the chosen slots, EUR.
    pub expected_cost_eur: f64,
}

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
    /// AC kWh the cheap band could not cover (full quarter-hours, not seconds left in the
    /// current slot); zero means a complete charge.
    pub missing_kwh: f64,
    /// Cause of a partial charge: `not_enough_eligible_slots` or `insufficient_capacity`.
    pub partial_reason: Option<&'static str>,
    /// Cheap-band price ceiling used for this plan, EUR/kWh. Carried forward so a later replan
    /// cannot demote a slot that was already booked just because the remaining-window median fell.
    pub band_ceil_eur_per_kwh: f64,
}

impl FullChargeAssignment {
    /// Whether this assignment reaches the whole remaining requirement.
    ///
    /// Only complete assignments may start a balancing dwell or stamp history.
    pub fn is_complete(&self) -> bool {
        self.missing_kwh <= 1e-6
    }
}

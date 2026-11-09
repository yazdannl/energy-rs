//! How far the measured grid and PV have moved from what the plan expected.

use crate::dispatch::MeterReading;
use energy_ledger::plan::PlanSlot;

/// Measured grid and PV, and the plan's own expectation for this slot.
///
/// Both sides are watts. The comparison is against the *plan's* slot, not the previous tick: a
/// tick-to-tick delta lets a slow drift never cross the threshold, which is the failure mode this
/// trigger exists to avoid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerMove {
    /// Measured grid, W (positive importing).
    pub grid_w: Option<f64>,
    /// What the plan's slot expects the grid to be, W.
    pub plan_grid_w: Option<f64>,
    /// Measured PV, W.
    pub pv_w: Option<f64>,
    /// What the plan's slot expects PV to be, W.
    pub plan_pv_w: Option<f64>,
}

impl PowerMove {
    /// The move for this slot: what the plan expected, against what the meter just measured.
    pub fn against(slot: &PlanSlot, meter: &MeterReading) -> Self {
        // The slot has no grid field of its own: its grid is what its load, PV and battery add up
        // to, which is exactly the quantity the fleet is trying to hold.
        Self {
            grid_w: meter.grid_w,
            plan_grid_w: Some((slot.load_kw - slot.pv_kw - slot.battery_kw) * 1000.0),
            pv_w: meter.pv_kw.map(|kw| kw * 1000.0),
            plan_pv_w: Some(slot.pv_kw * 1000.0),
        }
    }

    /// No measurement this tick: the caller keeps the plan.
    pub const fn none() -> Self {
        Self {
            grid_w: None,
            plan_grid_w: None,
            pv_w: None,
            plan_pv_w: None,
        }
    }

    /// Whether either input moved at least its own threshold (the threshold is inclusive: 10 W is
    /// the smallest step we call a change, 9 W is noise).
    pub fn crossed(&self, grid_react_w: f64, solar_react_w: f64) -> bool {
        self.grid_moved_by()
            .is_some_and(|move_w| move_w >= grid_react_w.max(0.0))
            || self
                .pv_moved_by()
                .is_some_and(|move_w| move_w >= solar_react_w.max(0.0))
    }

    /// Grid deviation, W.
    pub fn grid_moved_by(&self) -> Option<f64> {
        deviation(self.grid_w, self.plan_grid_w)
    }

    /// PV deviation, W.
    pub fn pv_moved_by(&self) -> Option<f64> {
        deviation(self.pv_w, self.plan_pv_w)
    }
}

fn deviation(measured: Option<f64>, planned: Option<f64>) -> Option<f64> {
    let (measured, planned) = (measured?, planned?);
    if !measured.is_finite() || !planned.is_finite() {
        return None;
    }
    Some((measured - planned).abs())
}

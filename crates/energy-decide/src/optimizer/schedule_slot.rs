//! One emitted slot of an accepted optimizer schedule.

/// Exact physical and financial evidence for one solved slot.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleSlot {
    /// Zero-based slot index.
    pub slot: usize,
    /// Slot start, UTC epoch seconds.
    pub t_start: i64,
    /// Canonical aggregate power, W (positive = charge).
    pub power_w: i32,
    /// Stored energy before the slot, kWh.
    pub energy_start_kwh: f64,
    /// Stored energy after the slot, kWh.
    pub energy_end_kwh: f64,
    /// Actually curtailed PV, kW.
    pub curtail_kw: f64,
    /// Settled grid power after curtailment, kW.
    pub grid_kw: f64,
    /// Grid cost, EUR.
    pub grid_eur: f64,
    /// Wear, EUR.
    pub wear_eur: f64,
    /// PV policy preference, EUR, reported separately.
    pub policy_preference_eur: f64,
}

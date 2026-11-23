//! One quarter-hour of an already-solved timeline; M3 does not solve or resample it.

use crate::plan_assemble::PackPower;

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineSlot {
    /// Slot start, UTC epoch seconds.
    pub t_start: i64,
    /// Stored energy at slot start, kWh.
    pub e_start_kwh: f64,
    /// Stored energy after the slot's battery power, kWh.
    pub e_end_kwh: f64,
    /// Battery power, charge positive, W.
    pub battery_w: f64,
    /// Net site grid power, import positive, W.
    pub grid_w: f64,
    /// Available PV generation, W.
    pub pv_w: f64,
    /// Non-battery house demand, W.
    pub load_w: f64,
    /// Per-pack requested powers, W.
    pub pack_powers: Vec<PackPower>,
    /// Whether cell hard limits are satisfied in this slot.
    pub cell_ok: bool,
    /// Whether thermal hard limits are satisfied in this slot.
    pub thermal_ok: bool,
    /// PV headroom available for a required grid charge in this slot, kWh.
    pub pv_headroom_kwh: f64,
}

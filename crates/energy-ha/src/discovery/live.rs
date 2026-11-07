//! One live snapshot, in W (and % for SoC).

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Live {
    /// Positive = import.
    pub grid_w: Option<f64>,
    /// Positive = production.
    pub pv_w: Option<f64>,
    /// Positive = discharge.
    pub battery_w: Option<f64>,
    /// `grid + pv + battery`; clamped to zero with [`Self::load_clamped`] when negative.
    pub load_w: Option<f64>,
    pub load_clamped: bool,
    /// Capacity-weighted battery SoC, 0–100.
    pub soc_pct: Option<f64>,
    /// Per-device positive W, keyed by name (falling back to the consumption statistic).
    pub devices_w: BTreeMap<String, f64>,
}

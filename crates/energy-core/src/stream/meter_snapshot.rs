//! Live power at the site meter.

/// The current grid/PV/load/battery power, all W and signed by flow direction.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeterSnapshot {
    /// Reading instant, UTC epoch seconds.
    pub at: i64,
    /// Grid power, W; positive imports.
    pub grid_w: f64,
    /// PV production, W (never negative).
    pub pv_w: f64,
    /// House load, W.
    pub load_w: f64,
    /// Battery AC power, W; positive charges the packs.
    pub battery_w: Option<f64>,
}

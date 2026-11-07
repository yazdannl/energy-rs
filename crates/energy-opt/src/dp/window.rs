//! The per-slot power windows a solve reports.

/// The two power windows `power_bounds_kw` intersected for one slot, in kW.
///
/// `soc_lo_kw ..= soc_hi_kw` is the SoC-reachable envelope, `grid_lo_kw ..= grid_hi_kw` the
/// grid-feasible one.  Kept per slot so a caller can name the constraint the chosen power sits on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    /// SoC-reachable lower bound, kW.
    pub soc_lo_kw: f64,
    /// SoC-reachable upper bound, kW.
    pub soc_hi_kw: f64,
    /// Grid-feasible lower bound, kW.
    pub grid_lo_kw: f64,
    /// Grid-feasible upper bound, kW.
    pub grid_hi_kw: f64,
}

impl Window {
    /// `[soc_lo, soc_hi, grid_lo, grid_hi]`, the order the old core returned.
    pub fn values(&self) -> [f64; 4] {
        [
            self.soc_lo_kw,
            self.soc_hi_kw,
            self.grid_lo_kw,
            self.grid_hi_kw,
        ]
    }
}

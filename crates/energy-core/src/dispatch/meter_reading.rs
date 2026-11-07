//! One read of the grid meter and the site's measured flows.

/// The meter as the grid-follow and grid-limit clauses see it.
///
/// `grid_w` is positive while importing. A missing value is kept as `None` -- never zero-filled --
/// because the ladder fails closed when it has neither a meter nor a forecast to bound a write.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MeterReading {
    /// Grid power, W (positive import).
    pub grid_w: Option<f64>,
    /// Measured house load, kW.
    pub load_kw: Option<f64>,
    /// Measured PV production, kW.
    pub pv_kw: Option<f64>,
}

impl MeterReading {
    /// A reading with only the grid meter.
    pub fn grid(grid_w: f64) -> Self {
        Self {
            grid_w: Some(grid_w),
            ..Self::default()
        }
    }

    /// The battery-idle grid base, in kW: measured load minus PV when both are known.
    pub fn base_kw(&self) -> Option<f64> {
        match (self.load_kw, self.pv_kw) {
            (Some(load), Some(pv)) => Some(load - pv),
            _ => None,
        }
    }
}

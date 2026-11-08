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

    /// Fill a load the site model could not balance from the fleet's measured pack power.
    ///
    /// The energy dashboard only has a battery power entity when the user configured one, and
    /// without that term `grid + pv` is not the house: a fleet discharging 2.8 kW into a house
    /// drawing 3 kW reads as a 0.16 kW house, and the ladder then bounds its writes on a base it
    /// does not have. The packs measure their own AC power, so the engine can close the balance
    /// itself. Pack power is positive while charging, so it comes out of the load.
    ///
    /// A load the site model already resolved is never replaced, and a missing grid or PV stays
    /// missing: a partial balance is not a measurement.
    #[must_use]
    pub fn with_pack_power(mut self, pack_w: Option<f64>) -> Self {
        if self.load_kw.is_none()
            && let (Some(grid), Some(pv), Some(pack)) = (self.grid_w, self.pv_kw, pack_w)
        {
            self.load_kw = Some(((grid + pv * 1000.0 - pack) / 1000.0).max(0.0));
        }
        self
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::MeterReading;

    fn reading(grid: Option<f64>, load: Option<f64>, pv: Option<f64>) -> MeterReading {
        MeterReading {
            grid_w: grid,
            load_kw: load,
            pv_kw: pv,
        }
    }

    #[test]
    fn a_discharging_fleet_becomes_house_load() {
        let meter = reading(Some(161.0), None, Some(0.0)).with_pack_power(Some(-2812.0));
        assert_eq!(meter.load_kw, Some(2.973));
    }

    #[test]
    fn a_charging_fleet_comes_out_of_the_load() {
        let meter = reading(Some(100.0), None, Some(3.0)).with_pack_power(Some(2000.0));
        assert_eq!(meter.load_kw, Some(1.1));
    }

    #[test]
    fn a_measured_load_is_never_replaced() {
        let meter = reading(Some(100.0), Some(0.9), Some(3.0)).with_pack_power(Some(2000.0));
        assert_eq!(meter.load_kw, Some(0.9));
    }

    #[test]
    fn a_partial_balance_stays_missing() {
        assert_eq!(
            reading(Some(100.0), None, None)
                .with_pack_power(Some(-500.0))
                .load_kw,
            None
        );
        assert_eq!(
            reading(None, None, Some(1.0))
                .with_pack_power(Some(-500.0))
                .load_kw,
            None
        );
        assert_eq!(
            reading(Some(100.0), None, Some(0.0))
                .with_pack_power(None)
                .load_kw,
            None
        );
    }

    #[test]
    fn an_exporting_house_clamps_to_zero() {
        let meter = reading(Some(-900.0), None, Some(0.2)).with_pack_power(Some(0.0));
        assert_eq!(meter.load_kw, Some(0.0));
    }
}

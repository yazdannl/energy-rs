//! One slot of the scenario, copied out of the input arrays.

use crate::dp::SolveInput;

/// The four prices/powers and the duration of one slot.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SlotSample {
    /// Forecast PV power, kW.
    pub(crate) pv_kw: f64,
    /// Forecast house load, kW.
    pub(crate) load_kw: f64,
    /// Import price, EUR/kWh.
    pub(crate) buy: f64,
    /// Export price, EUR/kWh.
    pub(crate) sell: f64,
    /// Modelled duration, minutes.
    pub(crate) minutes: f64,
}

impl SlotSample {
    /// Slot `index` of `input`.
    pub(crate) fn at(input: &SolveInput<'_>, index: usize) -> Self {
        Self {
            pv_kw: input.pv_kw[index],
            load_kw: input.load_kw[index],
            buy: input.buy[index],
            sell: input.sell[index],
            minutes: input.minutes[index],
        }
    }

    /// The slot's modelled duration, hours.
    pub(crate) fn dt(&self) -> f64 {
        self.minutes / 60.0
    }

    /// Load minus PV, kW.
    pub(crate) fn base(&self) -> f64 {
        self.load_kw - self.pv_kw
    }

    /// PV the house cannot use, kW.
    pub(crate) fn surplus(&self) -> f64 {
        (self.pv_kw - self.load_kw).max(0.0)
    }
}

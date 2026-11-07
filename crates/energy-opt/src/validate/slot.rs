//! One slot of the scenario, as the reference sees it.

use crate::dp::SolveInput;

/// Everything the reference needs from one slot (the `dp.SlotInput` fields the solver reads).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    /// Forecast PV power, kW.
    pub pv_kw: f64,
    /// Forecast house load, kW.
    pub load_kw: f64,
    /// Import price, EUR/kWh.
    pub buy: f64,
    /// Export price, EUR/kWh; `None` is priced as zero revenue, never as "free export".
    pub sell: f64,
    /// Modelled duration of the slot, minutes.
    pub minutes: f64,
}

impl Slot {
    /// The slot's modelled duration, hours.
    pub(crate) fn dt(&self) -> f64 {
        self.minutes / 60.0
    }

    /// Load minus PV: the grid power the slot needs before the battery moves.
    pub(crate) fn base(&self) -> f64 {
        self.load_kw - self.pv_kw
    }

    /// PV energy the house cannot use (the charge that forgoes export).
    pub(crate) fn surplus(&self) -> f64 {
        (self.pv_kw - self.load_kw).max(0.0)
    }
}

/// The per-slot input slice, or a typed error when the arrays disagree or contain `NaN`.
pub(crate) fn slot_at(input: &SolveInput<'_>, index: usize) -> Slot {
    Slot {
        pv_kw: input.pv_kw[index],
        load_kw: input.load_kw[index],
        buy: input.buy[index],
        sell: input.sell[index],
        minutes: input.minutes[index],
    }
}

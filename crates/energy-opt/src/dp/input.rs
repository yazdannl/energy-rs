//! The scenario arrays one solve takes.

/// The five flat arrays one solve needs.  All of them must have the same length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolveInput<'a> {
    /// Forecast PV power per slot, kW.
    pub pv_kw: &'a [f64],
    /// Forecast house load per slot, kW.
    pub load_kw: &'a [f64],
    /// Import price per slot, EUR/kWh.
    pub buy: &'a [f64],
    /// Export price per slot, EUR/kWh (unknown export is `0.0`, never "free").
    pub sell: &'a [f64],
    /// Modelled duration per slot, minutes.
    pub minutes: &'a [f64],
}

impl<'a> SolveInput<'a> {
    pub fn new(
        pv_kw: &'a [f64],
        load_kw: &'a [f64],
        buy: &'a [f64],
        sell: &'a [f64],
        minutes: &'a [f64],
    ) -> Self {
        Self {
            pv_kw,
            load_kw,
            buy,
            sell,
            minutes,
        }
    }

    /// The number of slots.
    pub fn len(&self) -> usize {
        self.pv_kw.len()
    }

    /// True when the scenario has no slots (a legal, degenerate solve).
    pub fn is_empty(&self) -> bool {
        self.pv_kw.is_empty()
    }
}

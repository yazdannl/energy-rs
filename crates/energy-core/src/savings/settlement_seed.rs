//! The saldering counters a day starts from.

use energy_price::{Saldering, Settlement};

/// The year-to-date metering a day's replay starts from.
///
/// `budget` is the annual saldering state at local midnight before the day; `None` means the meter
/// history for the year could not be read, so only the day's own imports create budget (a documented
/// lower bound, never a made-up number). `bonus_ytd_kwh` seeds the Zonnebonus counter so a day does
/// not re-earn the bonus on exports the year already counted.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SettlementSeed {
    pub budget: Option<Saldering>,
    pub bonus_ytd_kwh: f64,
}

impl SettlementSeed {
    /// A seed with the given budget and bonus counter.
    pub const fn new(budget: Option<Saldering>, bonus_ytd_kwh: f64) -> Self {
        Self {
            budget,
            bonus_ytd_kwh,
        }
    }

    /// The import meter at the start of the day (zero when the budget is unknown).
    pub fn import_ytd_kwh(&self) -> f64 {
        self.budget.map_or(0.0, |budget| budget.import_ytd_kwh)
    }

    /// Install the seed on the settlement used for a day in `year`.
    pub fn apply(&self, settlement: &mut Settlement, year: i32) {
        settlement.set_budget(self.budget);
        settlement.set_bonus_ytd_kwh(year, self.bonus_ytd_kwh);
    }
}

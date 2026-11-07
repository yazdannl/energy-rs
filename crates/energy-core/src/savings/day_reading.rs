//! What one local day needs from the outside world: measurements, prices and the budget seed.

use std::collections::BTreeMap;

use energy_price::Slot;

use super::{MeasuredDay, SettlementSeed};

/// One local day's measured flows, its published prices and the saldering counters it starts from.
///
/// `prices` is keyed by slot start. A missing key is a price gap: the day can still report its
/// measured energy, but it cannot be settled and therefore carries `NULL` money.
#[derive(Debug, Clone, PartialEq)]
pub struct DayReading {
    pub measured: MeasuredDay,
    pub prices: BTreeMap<i64, Slot>,
    pub seed: SettlementSeed,
}

impl DayReading {
    /// The published tariff slot for a measurement slot.
    pub fn price(&self, t_start: i64) -> Option<&Slot> {
        self.prices.get(&t_start)
    }

    /// Flat reference price for the arbitrage split: the mean all-in import price of the day's
    /// published slots. Zero when the day has no prices (the split is then meaningless and the day
    /// incomplete anyway).
    pub fn reference_price(&self) -> f64 {
        if self.prices.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.prices.values().map(|slot| slot.buy_all_in).sum();
        sum / self.prices.len() as f64
    }
}

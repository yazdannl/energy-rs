//! Where a day's measurements, prices and budget seed come from.

use chrono::NaiveDate;

use crate::dispatch::BoxFuture;

use super::{DayReading, SavingsError};

/// The read side of the savings rollup.
///
/// The real implementation is [`HaSavingsSource`](crate::savings::HaSavingsSource) over the energy
/// dashboard statistics and the price store; tests script it so every edge (a gap, a missing price,
/// a spent saldering budget) is reachable without a Home Assistant.
pub trait SavingsSource: Send + Sync {
    /// Measured quarter-hour flows, published prices and the saldering seed for one local day.
    fn read_day(&self, day: NaiveDate) -> BoxFuture<'_, Result<DayReading, SavingsError>>;
}

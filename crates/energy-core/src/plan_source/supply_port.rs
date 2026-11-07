//! The read side of the plan source.

use crate::dispatch::BoxFuture;
use crate::plan::PlanError;
use crate::plan_source::Supply;

/// Where a plan's upstream inputs come from.
///
/// The real implementation is [`HaPlanSupply`](crate::plan_source::HaPlanSupply); tests script a
/// supply so every edge case (short price book, statistics gap, missing forecast) is reachable
/// without a Home Assistant.
pub trait PlanSupply: Send + Sync {
    /// Read the inputs valid at `now` (UTC epoch seconds).
    fn supply(&self, now: i64) -> BoxFuture<'_, Result<Supply, PlanError>>;
}

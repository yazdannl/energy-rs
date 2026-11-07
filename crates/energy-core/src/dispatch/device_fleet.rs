//! The read side of the battery fleet.

use crate::dispatch::BoxFuture;
use crate::dispatch::fleet_error::FleetError;
use crate::dispatch::fleet_view::FleetView;

/// Everything a tick needs to know about the packs.
///
/// The real implementation is [`BatteryFleet`](crate::dispatch::BatteryFleet); tests answer a
/// scripted view. A failed read is an error, not an empty fleet: holding four packs because a
/// serial number vanished is how a fleet silently stops charging.
pub trait DeviceFleet: Send + Sync {
    /// Read every pack as of `now` (UTC epoch seconds).
    fn view(&self, now: i64) -> BoxFuture<'_, Result<FleetView, FleetError>>;
}

//! The write side of the battery fleet.

use crate::dispatch::BoxFuture;
use crate::dispatch::sink_error::SinkError;
use crate::dispatch::write_receipt::WriteReceipt;

/// Where commands reach the hardware.
///
/// The real implementation ([`BatterySink`](crate::dispatch::BatterySink)) wraps
/// `energy-battery`'s per-device controller, so pacing (>= 2 s per device, one command in flight),
/// clamping and echo verification happen there and this trait only reports the outcome. Tests use a
/// fake that records the command and can fail, refuse to echo, or echo a different value.
pub trait CommandSink: Send + Sync {
    /// Send `watts` (positive charging) to one pack.
    fn write(&self, pack_id: &str, watts: f64) -> BoxFuture<'_, Result<WriteReceipt, SinkError>>;

    /// Send the neutral hold (both limits 0, no `acMode`) to one pack.
    fn hold(&self, pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>>;
}

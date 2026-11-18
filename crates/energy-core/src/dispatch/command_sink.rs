//! The write side of the battery fleet.

use crate::dispatch::sink_error::SinkError;
use crate::dispatch::write_receipt::WriteReceipt;
use crate::dispatch::{BoxFuture, Progress};

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

    /// Write, advancing the watchdog as the transport completes each step.
    ///
    /// A sink with no intermediate wait keeps the default (one plain write); the dispatcher marks
    /// the heartbeat once when the command returns. A sink that paces a write and then polls for
    /// its echo overrides this so the wait cannot look like a stall.
    fn write_with_progress<'a>(
        &'a self,
        pack_id: &str,
        watts: f64,
        _progress: Option<&'a dyn Progress>,
    ) -> BoxFuture<'a, Result<WriteReceipt, SinkError>> {
        self.write(pack_id, watts)
    }

    /// Hold, advancing the watchdog as the transport completes each step.
    fn hold_with_progress<'a>(
        &'a self,
        pack_id: &str,
        _progress: Option<&'a dyn Progress>,
    ) -> BoxFuture<'a, Result<WriteReceipt, SinkError>> {
        self.hold(pack_id)
    }
}

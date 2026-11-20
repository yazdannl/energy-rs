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

    /// Write, with the tick's heartbeat so a bounded echo wait can mark progress.
    ///
    /// A sink with no intermediate wait keeps the default (one plain write); the dispatcher marks
    /// the heartbeat once when the command returns. A sink that paces a write and then polls for
    /// its echo overrides this so the wait cannot look like a stall (see
    /// [`BatterySink`](crate::dispatch::BatterySink)).
    fn write_with_progress<'a>(
        &'a self,
        pack_id: &str,
        watts: f64,
        progress: Option<&'a dyn Progress>,
        now: i64,
    ) -> BoxFuture<'a, Result<WriteReceipt, SinkError>> {
        let _ = (progress, now);
        self.write(pack_id, watts)
    }

    /// Hold, with the tick's heartbeat.
    fn hold_with_progress<'a>(
        &'a self,
        pack_id: &str,
        progress: Option<&'a dyn Progress>,
        now: i64,
    ) -> BoxFuture<'a, Result<WriteReceipt, SinkError>> {
        let _ = (progress, now);
        self.hold(pack_id)
    }
}

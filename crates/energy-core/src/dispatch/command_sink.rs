//! The write side of the battery fleet.

use crate::dispatch::BoxFuture;
use crate::dispatch::progress::Progress;
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

    /// The same write, with the tick's heartbeat so a bounded echo wait can mark progress.
    ///
    /// The default ignores the hook; sinks that wait for reports override it (see
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

    /// The same hold, with the tick's heartbeat.
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

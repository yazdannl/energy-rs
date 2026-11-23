//! Hard bounds on the write path so one unresponsive device cannot freeze the fleet.

use std::future::Future;
use std::time::Duration;

use crate::error::BatteryError;
use crate::transport::WriteAck;

/// Bound on one transport write. The pacer slot and the read/write transaction are held across this
/// call in `DeviceController::apply`; a device that accepts a request but never answers must not freeze
/// the sampler or every later hold. The HTTP transport already uses the same 5 s bound; this one also
/// covers a custom transport that forgets it.
pub const WRITE_TRANSPORT_TIMEOUT: Duration = Duration::from_secs(5);

/// A poll never waits for a write in flight; it returns a typed busy error and keeps the cached
/// report, so the sampler loop cannot die behind one slow command.
pub(super) const POLL_BUSY_TIMEOUT: Duration = Duration::from_millis(250);

/// Resolve a write by its hard deadline, never by the device's goodwill.
pub(super) async fn bounded_write(
    sn: &str,
    write: impl Future<Output = Result<WriteAck, BatteryError>>,
) -> Result<WriteAck, BatteryError> {
    tokio::time::timeout(WRITE_TRANSPORT_TIMEOUT, write)
        .await
        .unwrap_or_else(|_| {
            Err(BatteryError::Invalid {
                reason: format!(
                    "device {sn} write transport exceeded {}s",
                    WRITE_TRANSPORT_TIMEOUT.as_secs()
                ),
            })
        })
}

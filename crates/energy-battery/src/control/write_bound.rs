//! Hard bounds on device I/O so one unresponsive pack cannot freeze the fleet.

use std::future::Future;
use std::time::Duration;

use crate::control::fault::ControlFault;
use crate::error::BatteryError;
use crate::report::Report;
use crate::transport::WriteAck;

/// Bound on one transport write. The pacer slot and the read/write transaction are held across this
/// call in `DeviceController::apply`; a device that accepts a request but never answers must not freeze
/// the sampler or every later hold. The HTTP transport already uses the same 5 s bound; this one also
/// covers a custom transport that forgets it.
pub const WRITE_TRANSPORT_TIMEOUT: Duration = Duration::from_secs(5);

/// Bound on one report read. A hung read held the transaction that `apply` needs, so it could stop a
/// safety hold exactly like a hung write. Reads get the same hard deadline for the same reason.
pub const READ_TRANSPORT_TIMEOUT: Duration = Duration::from_secs(5);

/// Bound on acquiring the transaction for a write. Slightly longer than a bounded read so an
/// in-flight report gets a chance to finish before the writer gives up and the hold proceeds.
pub const WRITE_TRANSACTION_TIMEOUT: Duration = Duration::from_secs(6);

/// A poll never waits for a write in flight; it returns a typed busy error and keeps the cached
/// report, so the sampler loop cannot die behind one slow command.
pub(super) const POLL_BUSY_TIMEOUT: Duration = Duration::from_millis(250);

/// Resolve a write by its hard deadline, never by the device's goodwill.
///
/// A timeout is not a "no": the device may already have applied the command. It keeps the typed
/// uncertain fault and the pending request so no ordinary write resumes until a hold is verified.
pub(super) async fn bounded_write(
    sn: &str,
    request_id: u32,
    write: impl Future<Output = Result<WriteAck, BatteryError>>,
) -> Result<WriteAck, BatteryError> {
    tokio::time::timeout(WRITE_TRANSPORT_TIMEOUT, write)
        .await
        .unwrap_or_else(|_| {
            Err(BatteryError::Fault(ControlFault::UncertainWrite {
                sn: sn.to_owned(),
                request_id,
                detail: format!("no response within {}s", WRITE_TRANSPORT_TIMEOUT.as_secs()),
            }))
        })
}

/// Resolve a report read by its hard deadline, so the transaction always comes back.
pub(super) async fn bounded_report(
    sn: &str,
    report: impl Future<Output = Result<Report, BatteryError>>,
) -> Result<Report, BatteryError> {
    tokio::time::timeout(READ_TRANSPORT_TIMEOUT, report)
        .await
        .unwrap_or_else(|_| {
            Err(BatteryError::Invalid {
                reason: format!(
                    "device {sn} report read exceeded {}s",
                    READ_TRANSPORT_TIMEOUT.as_secs()
                ),
            })
        })
}

//! Bounded echo waiting and safety preemption for one controller.

use tokio::time::Instant;

use crate::control::controller::{DeviceController, SAFETY_HOLD_WINDOW};
use crate::control::echo::ECHO_POLL_INTERVAL;
use crate::control::fault::ControlFault;
use crate::control::observation::Observation;
use crate::error::BatteryError;
use crate::transport::DeviceIo;

/// Keep reading until the target command confirms, faults, is replaced, or the caller's deadline wins.
///
/// One read future stays pinned across heartbeat beats: a beat marks progress and re-arms, it does
/// **not** cancel the read. Only the hard deadline in [`poll_confirming`](DeviceController::poll_confirming)
/// or a transport error ends an in-flight read. A transport error resolves the command at once so no
/// broken read can hold up a safety hold; if a hold already replaced the command, the result is marked
/// superseded instead of becoming a fault.
pub(super) async fn confirm_loop<T: DeviceIo>(
    controller: &DeviceController<T>,
    target: u32,
    on_progress: &impl Fn(),
) -> Result<Observation, BatteryError> {
    on_progress();
    loop {
        // One read per poll stays pinned until it finishes; a beat only re-arms, never replaces it.
        let mut read = Box::pin(controller.poll());
        let mut beat = Box::pin(tokio::time::sleep(ECHO_POLL_INTERVAL));
        let observation = loop {
            tokio::select! {
                result = &mut read => break match result {
                    Ok(observation) => observation,
                    Err(error) => {
                        return expire(controller, target, &format!("report read failed: {error}"));
                    }
                },
                () = &mut beat => {
                    on_progress();
                    beat = Box::pin(tokio::time::sleep(ECHO_POLL_INTERVAL));
                }
            }
        };
        on_progress();
        // A confirming or faulting read clears `pending` itself, so check those first.
        if observation.confirmed.is_some() || observation.fault.is_some() {
            return Ok(observation);
        }
        if controller.pending().map(|pending| pending.request_id) != Some(target) {
            return Ok(Observation::superseded(observation.report));
        }
        // Pace the next read; a ready transport must not spin and starve the deadline.
        tokio::time::sleep(ECHO_POLL_INTERVAL).await;
    }
}

/// A neutralising hold clears an unresolved command within [`SAFETY_HOLD_WINDOW`].
///
/// The pacer slot is already claimed by the hold's `apply`, so no ordinary writer can slip in while
/// this waits. Each read-and-cancel is one locked operation, and the loop uses an absolute deadline,
/// so a racing replacement cannot extend the bound indefinitely.
pub(super) async fn clear_for_hold<T: DeviceIo>(controller: &DeviceController<T>) {
    let deadline = Instant::now() + SAFETY_HOLD_WINDOW;
    loop {
        let Some(wait) = controller.clear_pending_for_hold(deadline) else {
            return;
        };
        tokio::time::sleep(wait).await;
    }
}

/// Resolve exactly the command this waiter was created for, with one typed timeout fault.
///
/// When the target was already cancelled or replaced by a safety hold, this is a *supersession*, not
/// a failure: the caller must not send a second fail-safe hold for the same episode.
pub(super) fn expire<T: DeviceIo>(
    controller: &DeviceController<T>,
    request_id: u32,
    detail: &str,
) -> Result<Observation, BatteryError> {
    let sn = controller.sn().to_owned();
    let fallback = timeout_fallback(controller, request_id, detail, &sn);
    if let Some((fault, report)) = controller.expire_pending(request_id, fallback) {
        tracing::warn!(sn, request_id, %fault, "confirmation expired");
        return Ok(Observation {
            report,
            confirmed: None,
            fault: Some(fault),
            superseded: false,
        });
    }
    superseded_observation(controller, request_id, &sn)
}

fn timeout_fallback<T: DeviceIo>(
    controller: &DeviceController<T>,
    request_id: u32,
    detail: &str,
    sn: &str,
) -> ControlFault {
    let window_ms = controller
        .pending()
        .filter(|pending| pending.request_id == request_id)
        .map(|pending| u64::try_from(pending.age(Instant::now()).as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default();
    ControlFault::ConfirmationTimeout {
        window_ms,
        detail: detail.to_owned(),
        sn: sn.to_owned(),
        request_id,
    }
}

fn superseded_observation<T: DeviceIo>(
    controller: &DeviceController<T>,
    request_id: u32,
    sn: &str,
) -> Result<Observation, BatteryError> {
    let report = controller
        .last_report()
        .ok_or_else(|| BatteryError::Invalid {
            reason: format!("device {sn} has no report to return"),
        })?;
    tracing::debug!(sn, request_id, "waiter superseded by a safety hold");
    Ok(Observation::superseded(report))
}

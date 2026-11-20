//! Bounded echo waiting and safety preemption for one controller.

use tokio::time::Instant;

use crate::control::controller::{DeviceController, SAFETY_HOLD_WINDOW};
use crate::control::echo::ECHO_POLL_INTERVAL;
use crate::control::fault::ControlFault;
use crate::control::observation::Observation;
use crate::error::BatteryError;
use crate::transport::DeviceIo;

/// Keep reading until the target command confirms, faults, or is replaced.
///
/// A stalled read yields to the heartbeat every `ECHO_POLL_INTERVAL`; the caller's hard deadline
/// still bounds the whole loop. A read error resolves the command at once so no broken transport
/// can hold up a safety hold.
pub(super) async fn confirm_loop<T: DeviceIo>(
    controller: &DeviceController<T>,
    target: u32,
    on_progress: &impl Fn(),
) -> Result<Observation, BatteryError> {
    loop {
        on_progress();
        let mut read = Box::pin(controller.poll());
        let mut beat = Box::pin(tokio::time::sleep(ECHO_POLL_INTERVAL));
        let observation = tokio::select! {
            result = &mut read => match result {
                Ok(observation) => observation,
                Err(error) => {
                    return expire(controller, target, &format!("report read failed: {error}"));
                }
            },
            () = &mut beat => {
                on_progress();
                continue;
            }
        };
        if controller.pending().map(|pending| pending.request_id) != Some(target)
            || observation.confirmed.is_some()
            || observation.fault.is_some()
        {
            return Ok(observation);
        }
        on_progress();
        tokio::time::sleep(ECHO_POLL_INTERVAL).await;
    }
}

/// A neutralising hold waits at most [`SAFETY_HOLD_WINDOW`], then cancels the current command.
pub(super) async fn clear_for_hold<T: DeviceIo>(controller: &DeviceController<T>) {
    loop {
        let Some(pending) = controller.pending() else {
            return;
        };
        let remaining = SAFETY_HOLD_WINDOW.saturating_sub(pending.age(Instant::now()));
        if remaining.is_zero() {
            controller.cancel(pending.request_id);
            return;
        }
        tokio::time::sleep(remaining).await;
    }
}

/// Resolve exactly the command this waiter was created for, with one typed timeout fault.
pub(super) fn expire<T: DeviceIo>(
    controller: &DeviceController<T>,
    request_id: u32,
    detail: &str,
) -> Result<Observation, BatteryError> {
    let sn = controller.sn().to_owned();
    let Some(pending) = controller.pending().filter(|p| p.request_id == request_id) else {
        return Err(BatteryError::Invalid {
            reason: format!("command {request_id} was already resolved"),
        });
    };
    let fallback = ControlFault::ConfirmationTimeout {
        window_ms: u64::try_from(pending.age(Instant::now()).as_millis()).unwrap_or(u64::MAX),
        detail: detail.to_owned(),
        sn: sn.clone(),
        request_id,
    };
    let report = controller
        .last_report()
        .ok_or_else(|| BatteryError::Invalid {
            reason: format!("device {sn} has no report to return"),
        })?;
    let fault = controller
        .record_timeout(request_id, fallback)
        .ok_or_else(|| BatteryError::Invalid {
            reason: format!("command {request_id} was replaced before it expired"),
        })?;
    tracing::warn!(sn, request_id, %fault, "confirmation expired");
    Ok(Observation {
        report,
        confirmed: None,
        fault: Some(fault),
    })
}

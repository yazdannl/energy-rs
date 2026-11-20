//! Bounded echo waiting and safety preemption for one controller.

use std::time::Duration;

use tokio::time::Instant;

use crate::control::controller::{DeviceController, SAFETY_HOLD_WINDOW};
use crate::control::echo::ECHO_POLL_INTERVAL;
use crate::control::fault::ControlFault;
use crate::control::observation::Observation;
use crate::error::BatteryError;
use crate::transport::DeviceIo;

/// How the bounded loop stopped: with an observation, or at the hard deadline.
enum LoopEnd {
    Observation(Box<Observation>),
    Deadline,
}

/// Wait for the command this call was created for, inside one absolute deadline.
///
/// A read that misses is retried inside the window and ends as a read-miss observation, never as a
/// device fault; a command replaced by a safety hold resolves as superseded. The shared `read_error`
/// survives cancellation of the inner loop, so a deadline reached while reads were missing still
/// ends as a read miss rather than a device fault.
pub(super) async fn wait<T: DeviceIo>(
    controller: &DeviceController<T>,
    window: Duration,
    on_progress: &impl Fn(),
) -> Result<Observation, BatteryError> {
    let deadline = Instant::now() + window;
    let Some(target) = controller.pending().map(|pending| pending.request_id) else {
        return match tokio::time::timeout_at(deadline, controller.poll()).await {
            Ok(result) => result,
            Err(_) => Err(BatteryError::Invalid {
                reason: "report read exceeded the confirmation deadline".to_owned(),
            }),
        };
    };
    let mut read_error = None;
    match tokio::time::timeout_at(
        deadline,
        confirm_loop(controller, target, deadline, &mut read_error, on_progress),
    )
    .await
    {
        Ok(Ok(LoopEnd::Observation(observation))) => Ok(*observation),
        Ok(Ok(LoopEnd::Deadline)) | Err(_) => {
            if controller.pending().map(|pending| pending.request_id) != Some(target) {
                return superseded_observation(controller, target, controller.sn());
            }
            if read_error.is_some() {
                read_miss(controller, target, read_error)
            } else {
                expire(controller, target, "confirmation deadline")
            }
        }
        Ok(Err(error)) => Err(error),
    }
}

/// Keep reading until the target command confirms, faults, is replaced, or the deadline wins.
///
/// One read future stays pinned across heartbeat beats: a beat marks progress and re-arms, it does
/// **not** cancel the read. Only the caller's hard deadline or a transport error ends an in-flight
/// read. A terminal read is only returned to the waiter whose request id it resolved
/// ([`Observation::resolved_request`]); a hold that replaced the target resolves this waiter as
/// superseded instead.
async fn confirm_loop<T: DeviceIo>(
    controller: &DeviceController<T>,
    target: u32,
    deadline: Instant,
    read_error: &mut Option<String>,
    on_progress: &impl Fn(),
) -> Result<LoopEnd, BatteryError> {
    'wait: loop {
        on_progress();
        let mut read = Box::pin(controller.poll());
        let mut beat = Box::pin(tokio::time::sleep(ECHO_POLL_INTERVAL));
        let observation = loop {
            tokio::select! {
                result = &mut read => match result {
                    Ok(observation) => {
                        *read_error = None;
                        break Some(observation);
                    }
                    // A read miss says nothing about the write; retry until the hard deadline.
                    Err(error) => {
                        *read_error = Some(error.to_string());
                        on_progress();
                        if Instant::now() >= deadline {
                            return Ok(LoopEnd::Deadline);
                        }
                        tokio::time::sleep((deadline - Instant::now()).min(ECHO_POLL_INTERVAL))
                            .await;
                        break None;
                    }
                },
                () = &mut beat => {
                    on_progress();
                    beat = Box::pin(tokio::time::sleep(ECHO_POLL_INTERVAL));
                }
            }
        };
        let Some(observation) = observation else {
            continue 'wait;
        };
        on_progress();
        // A terminal read belongs to the request it resolved, never automatically to this waiter:
        // a hold that replaced the target may confirm or fault while our read is still pinned.
        if let Some(resolved) = observation.resolved_request {
            let observation = if resolved == target {
                observation
            } else {
                Observation::superseded(observation.report)
            };
            return Ok(LoopEnd::Observation(Box::new(observation)));
        }
        if observation.confirmed.is_some() || observation.fault.is_some() {
            return Ok(LoopEnd::Observation(Box::new(observation)));
        }
        if controller.pending().map(|pending| pending.request_id) != Some(target) {
            return Ok(LoopEnd::Observation(Box::new(Observation::superseded(
                observation.report,
            ))));
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(LoopEnd::Deadline);
        }
        tokio::time::sleep((deadline - now).min(ECHO_POLL_INTERVAL)).await;
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

/// Resolve a command whose echo could not be read, without reporting a device fault.
///
/// Cleanup is request-id scoped and happens before the report lookup: even a device with no cached
/// report must not leave a pending command that would refuse the next write as `Unconfirmed`.
pub(super) fn read_miss<T: DeviceIo>(
    controller: &DeviceController<T>,
    request_id: u32,
    detail: Option<String>,
) -> Result<Observation, BatteryError> {
    controller.cancel(request_id);
    let report = controller
        .last_report()
        .ok_or_else(|| BatteryError::NotRead {
            sn: controller.sn().to_owned(),
        })?;
    Ok(Observation {
        report,
        confirmed: None,
        fault: None,
        resolved_request: Some(request_id),
        superseded: false,
        read_error: Some(detail.unwrap_or_else(|| "report read missed".to_owned())),
    })
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
    // Release the command first: a missing cached report must not strand it as `Unconfirmed`.
    let Some(fault) = controller.record_timeout(request_id, fallback) else {
        return superseded_observation(controller, request_id, &sn);
    };
    let report = controller
        .last_report()
        .ok_or_else(|| BatteryError::Invalid {
            reason: format!("device {sn} has no report to return"),
        })?;
    tracing::warn!(sn, request_id, %fault, "confirmation expired");
    Ok(Observation {
        report,
        confirmed: None,
        fault: Some(fault),
        resolved_request: Some(request_id),
        superseded: false,
        read_error: None,
    })
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::control::command::Command;
    use crate::control::limits::ControlLimits;
    use crate::parse_report;
    use crate::report::Report;
    use crate::transport::{DeviceIo, WriteAck, WriteRequest};

    struct NoReport;

    impl DeviceIo for NoReport {
        fn sn(&self) -> &'static str {
            "NO-REPORT"
        }

        fn report(&self) -> crate::transport::BoxFuture<'_, Result<Report, BatteryError>> {
            Box::pin(std::future::ready(Err(BatteryError::Invalid {
                reason: "no cached report".to_owned(),
            })))
        }

        fn write(
            &self,
            _: WriteRequest,
        ) -> crate::transport::BoxFuture<'_, Result<WriteAck, BatteryError>> {
            Box::pin(std::future::ready(Ok(WriteAck {
                timestamp: None,
                message_id: None,
                code: 200,
                sn: Some(self.sn().to_owned()),
            })))
        }
    }

    fn report() -> Report {
        let document = json!({
            "timestamp": 1_791_959_470,
            "messageId": 4211,
            "sn": "NO-REPORT",
            "version": 3,
            "product": "solarFlow800Plus",
            "properties": {"acMode": 2, "inputLimit": 0, "outputLimit": 0},
            "packData": []
        });
        parse_report(&serde_json::to_vec(&document).expect("report json")).expect("parse")
    }

    #[test]
    fn a_read_miss_releases_the_command_even_with_no_cached_report() {
        let controller = DeviceController::new(NoReport, ControlLimits::unlimited());
        let mut inner = controller.lock().expect("inner");
        let pending =
            controller.install(&mut inner, Command::Charge { watts: 300 }, &report(), false);
        drop(inner);
        assert_eq!(
            controller.pending().map(|p| p.request_id),
            Some(pending.request_id)
        );

        assert!(
            read_miss(
                &controller,
                pending.request_id,
                Some("read timeout".to_owned())
            )
            .is_err()
        );
        assert_eq!(
            controller.pending(),
            None,
            "no Unconfirmed storm on the next write"
        );

        // The same cleanup guarantee applies to a timed-out expiry with no cached report.
        let mut inner = controller.lock().expect("inner");
        let pending =
            controller.install(&mut inner, Command::Charge { watts: 300 }, &report(), false);
        drop(inner);
        assert!(expire(&controller, pending.request_id, "deadline").is_err());
        assert_eq!(
            controller.pending(),
            None,
            "expiry must also release the command"
        );
    }
}

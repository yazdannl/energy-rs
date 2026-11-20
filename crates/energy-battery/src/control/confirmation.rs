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

/// Wait for one confirmation inside an absolute deadline.
///
/// The shared `read_error` survives cancellation of the inner loop, so a deadline reached while
/// reads were missing ends as a read miss rather than a device fault.
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
            if read_error.is_some() {
                read_miss(controller, target, read_error)
            } else {
                expire(controller, target, "confirmation deadline")
            }
        }
        Ok(Err(error)) => Err(error),
    }
}

/// Keep reading until the target command confirms, faults, or the deadline passes.
///
/// A stalled read yields to the heartbeat every `ECHO_POLL_INTERVAL`; the caller's hard deadline
/// bounds every read and the whole loop. A read error is retried inside that deadline and only
/// becomes a read-miss observation when nothing else can resolve the command, so a slow report
/// path never arms a transport backoff. The waiter is bound to its request id: it never resolves a
/// command that replaced the one it started with.
async fn confirm_loop<T: DeviceIo>(
    controller: &DeviceController<T>,
    target: u32,
    deadline: Instant,
    read_error: &mut Option<String>,
    on_progress: &impl Fn(),
) -> Result<LoopEnd, BatteryError> {
    loop {
        // The caller's `timeout_at(deadline, ..)` drops this future at the deadline, so no poll
        // can start past it; each individual read is bounded by the same instant too.
        on_progress();
        let mut read = Box::pin(tokio::time::timeout_at(deadline, controller.poll()));
        let mut beat = Box::pin(tokio::time::sleep(ECHO_POLL_INTERVAL));
        let observation = tokio::select! {
            result = &mut read => match result {
                Ok(Ok(observation)) => {
                    if read_error.is_some() {
                        *read_error = None;
                    }
                    observation
                },
                // A read miss says nothing about the write; retry until the hard deadline.
                Ok(Err(error)) => {
                    *read_error = Some(error.to_string());
                    on_progress();
                    if Instant::now() >= deadline {
                        return Ok(LoopEnd::Deadline);
                    }
                    tokio::time::sleep((deadline - Instant::now()).min(ECHO_POLL_INTERVAL)).await;
                    continue;
                },
                // No read error was observed: this is the absolute echo deadline, so let the
                // caller expire with the pending command's typed doubt, not a read-miss policy.
                Err(_) => return Ok(LoopEnd::Deadline),
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
            return Ok(LoopEnd::Observation(Box::new(observation)));
        }
        on_progress();
        let now = Instant::now();
        if now >= deadline {
            return Ok(LoopEnd::Deadline);
        }
        tokio::time::sleep((deadline - now).min(ECHO_POLL_INTERVAL)).await;
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
        read_error: Some(detail.unwrap_or_else(|| "report read missed".to_owned())),
    })
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
    // Release the command first: a missing cached report must not strand it as `Unconfirmed`.
    let fault = controller
        .record_timeout(request_id, fallback)
        .ok_or_else(|| BatteryError::Invalid {
            reason: format!("command {request_id} was replaced before it expired"),
        })?;
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
        read_error: None,
    })
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

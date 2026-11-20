//! Bounded echo wait: retry reads inside the window instead of faulting on a read miss.

use std::time::Duration;

use tokio::time::Instant;

use crate::control::controller::DeviceController;
use crate::control::echo::ECHO_POLL_INTERVAL;
use crate::control::observation::Observation;
use crate::error::BatteryError;
use crate::transport::DeviceIo;

/// Poll until the command is confirmed, faulted, or the window is spent.
///
/// A transport error is retried within the window: an accepted write whose report read timed out
/// is not evidence of a device fault. If reads keep missing, the command is released without a
/// fault and the miss is returned for the journal; the engine's latch keeps ownership.
pub(super) async fn confirm<T: DeviceIo>(
    controller: &DeviceController<T>,
    window: Duration,
    on_progress: impl Fn(),
) -> Result<Observation, BatteryError> {
    let deadline = Instant::now() + window;
    let mut observation: Option<Observation> = None;
    let mut read_error = None;
    loop {
        match controller.poll().await {
            Ok(fresh) => {
                if read_error.is_some() {
                    read_error = None;
                }
                if fresh.confirmed.is_some()
                    || fresh.fault.is_some()
                    || !controller.is_inconclusive()
                {
                    return Ok(fresh);
                }
                observation = Some(fresh);
            }
            // A read miss says nothing about the write. Retry until the bounded window ends
            // instead of turning one slow HTTP read into a device fault and a 10 s backoff.
            Err(error) => read_error = Some(error.to_string()),
        }
        let now = Instant::now();
        if now >= deadline {
            if read_error.is_some() {
                let report = observation
                    .map(|observation| observation.report)
                    .or_else(|| controller.last_report())
                    .ok_or_else(|| BatteryError::NotRead {
                        sn: controller.sn().to_owned(),
                    })?;
                // Nothing can confirm this command now, but leaving it pending would make the
                // next write fail as `Unconfirmed`.
                controller.release_unconfirmed();
                return Ok(Observation {
                    report,
                    confirmed: None,
                    fault: None,
                    read_error,
                });
            }
            let observation = observation.ok_or_else(|| BatteryError::NotRead {
                sn: controller.sn().to_owned(),
            })?;
            return Ok(controller.expire_inconclusive(observation));
        }
        on_progress();
        tokio::time::sleep((deadline - now).min(ECHO_POLL_INTERVAL)).await;
    }
}

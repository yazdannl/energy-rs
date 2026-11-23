//! The real command sink: `energy-battery`'s per-device controller, pacing and echo included.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use chrono::Utc;
use energy_battery::BatteryError;
use energy_battery::control::flow::{FLOW_THRESHOLD_W, measured_w, status_direction};
use energy_battery::control::{
    Command, DeviceController, Direction, ECHO_CONFIRM_WINDOW, ECHO_TOLERANCE_W, PACK_IO_BUDGET,
    mark_uncertain_write,
};
use energy_battery::transport::DeviceIo;

use crate::dispatch::{BoxFuture, CommandSink, Progress, SinkError, WriteReceipt, advance};

/// A [`CommandSink`] over the device controllers.
///
/// Every write goes through the controller, so the protocol rules hold by construction: at least
/// two seconds between writes to one device, one command in flight, `smartMode: 1` on every write,
/// no `acMode` on a hold, and confirmation only when the *next report* echoes the limit. A fault
/// the controller reports (wedge, restart, echo mismatch) is returned as a [`SinkError`], which the
/// dispatcher turns into exactly one fail-safe hold and a backoff.
pub struct BatterySink<T: DeviceIo> {
    devices: BTreeMap<String, Arc<DeviceController<T>>>,
    confirm_window: Duration,
    sample_fresh_after: Duration,
}

impl<T: DeviceIo> BatterySink<T> {
    /// A sink over the controllers, keyed by serial.
    pub fn new(devices: Vec<Arc<DeviceController<T>>>, sample_fresh_after: Duration) -> Self {
        Self {
            devices: devices
                .into_iter()
                .map(|device| (device.sn().to_owned(), device))
                .collect(),
            confirm_window: ECHO_CONFIRM_WINDOW,
            sample_fresh_after,
        }
    }

    /// Shorten the bounded waits (tests; production keeps the measured windows).
    #[must_use]
    pub fn with_confirm_window(mut self, window: Duration) -> Self {
        self.confirm_window = window;
        self
    }

    /// The serials this sink may command.
    pub fn pack_ids(&self) -> Vec<&str> {
        self.devices.keys().map(String::as_str).collect()
    }

    /// One end-to-end budget for every caller: dispatch, boot holds, shutdown holds and maintenance.
    /// A timeout marks the controller's write uncertain so only a verified hold can continue.
    async fn send<'a>(
        &'a self,
        pack_id: String,
        command: Command,
        progress: Option<&'a dyn Progress>,
        now: i64,
    ) -> Result<WriteReceipt, SinkError> {
        let controller = self
            .devices
            .get(&pack_id)
            .ok_or_else(|| SinkError::UnknownPack {
                pack_id: pack_id.clone(),
            })?;
        let phase = Arc::new(AtomicU8::new(1));
        let inner = self.send_inner(command, controller, &phase, progress, now);
        let Ok(result) = tokio::time::timeout(PACK_IO_BUDGET, inner).await else {
            let phase = phase_name(phase.load(Ordering::Relaxed)).to_owned();
            mark_uncertain_write(
                controller,
                format!("{phase} exceeded the {PACK_IO_BUDGET:?} pack budget"),
            );
            return Err(SinkError::Timeout { pack_id, phase });
        };
        result
    }

    /// Apply, then confirm only what the next bounded report proves.
    async fn send_inner<'a>(
        &'a self,
        command: Command,
        controller: &DeviceController<T>,
        phase: &AtomicU8,
        progress: Option<&'a dyn Progress>,
        now: i64,
    ) -> Result<WriteReceipt, SinkError> {
        let mark = || advance(progress, Utc::now().timestamp().max(now));
        let applied = match controller.apply(command).await {
            Ok(applied) => Some(applied),
            // A previous command still owns the device. The changed plan was NOT written; reporting
            // it as pending would leave the old setpoint running while the plan moves on.
            Err(BatteryError::Unconfirmed { sn }) => {
                tracing::warn!(%sn, "new setpoint blocked by an unconfirmed command");
                return Ok(WriteReceipt::blocked_by_pending());
            }
            Err(error) => return Err(device_error(controller.sn().to_owned(), &error)),
        };
        mark();
        let target = applied.map_or(command, |applied| applied.command);
        // A direction change used to wait the whole 45 s AC ramp inside a tick; with four packs
        // that alone outlives the watchdog. The write is accepted, so defer flow confirmation to
        // the next fleet read. Only a hold still needs the short echo wait inside this tick.
        if target.direction().is_some() {
            return Ok(WriteReceipt::pending());
        }
        phase.store(2, Ordering::Relaxed);
        let observation = controller
            .poll_confirming(self.confirm_window, &mark)
            .await
            .map_err(|error| device_error(controller.sn().to_owned(), &error))?;
        if observation.superseded {
            return Ok(WriteReceipt::superseded());
        }
        if let Some(reason) = observation.read_error {
            return Err(SinkError::ReadUnverified {
                pack_id: controller.sn().to_owned(),
                reason,
            });
        }
        if let Some(fault) = observation.fault {
            return Err(SinkError::Fault {
                pack_id: controller.sn().to_owned(),
                fault: Box::new(fault),
            });
        }
        Ok(receipt(
            target,
            &observation,
            Utc::now().timestamp(),
            self.sample_fresh_after,
        ))
    }
}

fn phase_name(phase: u8) -> &'static str {
    if phase == 2 { "echo" } else { "apply" }
}

impl<T: DeviceIo> CommandSink for BatterySink<T> {
    fn write(&self, pack_id: &str, watts: f64) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        let command = command_for(watts);
        Box::pin(self.send(pack_id.to_owned(), command, None, 0))
    }

    fn hold(&self, pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        Box::pin(self.send(pack_id.to_owned(), Command::Hold, None, 0))
    }

    fn write_with_progress<'a>(
        &'a self,
        pack_id: &str,
        watts: f64,
        progress: Option<&'a dyn Progress>,
        now: i64,
    ) -> BoxFuture<'a, Result<WriteReceipt, SinkError>> {
        Box::pin(self.send(pack_id.to_owned(), command_for(watts), progress, now))
    }

    fn hold_with_progress<'a>(
        &'a self,
        pack_id: &str,
        progress: Option<&'a dyn Progress>,
        now: i64,
    ) -> BoxFuture<'a, Result<WriteReceipt, SinkError>> {
        Box::pin(self.send(pack_id.to_owned(), Command::Hold, progress, now))
    }
}

/// A transport or read failure is an unreachable device, not a refusal the ladder can diagnose.
fn device_error(pack_id: String, error: &BatteryError) -> SinkError {
    match &error {
        BatteryError::Transport { .. }
        | BatteryError::Disabled { .. }
        | BatteryError::HttpStatus { .. }
        | BatteryError::Payload { .. }
        | BatteryError::Discovery { .. }
        | BatteryError::Invalid { .. } => SinkError::DeviceUnreachable {
            pack_id,
            reason: error.to_string(),
        },
        _ => SinkError::Device {
            pack_id,
            reason: error.to_string(),
        },
    }
}

/// A receipt from the observation, never from the clamped intention: an echoed limit with the AC
/// relay open must not be reported as applied power.
fn receipt(
    command: Command,
    observation: &energy_battery::Observation,
    now: i64,
    fresh_after: Duration,
) -> WriteReceipt {
    let verified = observation
        .confirmed
        .is_some_and(|confirmed| same_intent(confirmed, command));
    let partial = observation
        .partial
        .is_some_and(|confirmed| same_intent(confirmed, command));
    let report = &observation.report;
    let age = now.saturating_sub(report.timestamp);
    let fresh = age >= 0 && age <= i64::try_from(fresh_after.as_secs()).unwrap_or(i64::MAX);
    let measured = measured_w(&command, report).max(0) as f64;
    let idle = report.properties.grid_input_w.abs() <= FLOW_THRESHOLD_W
        && report.properties.output_home_w.abs() <= FLOW_THRESHOLD_W;
    let flow_measured = fresh
        && match command.direction() {
            Some(Direction::Discharge) => {
                measured > f64::from(i32::try_from(FLOW_THRESHOLD_W).unwrap_or(i32::MAX))
            }
            Some(Direction::Charge) => {
                measured > f64::from(i32::try_from(FLOW_THRESHOLD_W).unwrap_or(i32::MAX))
                    && status_direction(report.properties.ac_status, report.properties.dc_status)
                        == Some(Direction::Charge)
            }
            None => idle,
        };
    let applied_w = if flow_measured {
        match command.direction() {
            Some(Direction::Discharge) => -measured,
            Some(Direction::Charge) | None => measured,
        }
    } else {
        0.0
    };
    let result = if partial {
        WriteReceipt::partial(applied_w)
    } else {
        WriteReceipt {
            applied_w,
            verified,
            partial: false,
            superseded: false,
            flow_measured: false,
            measured_at: None,
            confirmation_pending: false,
            blocked_by_pending: false,
        }
    };
    if flow_measured {
        result.with_measurement(report.timestamp)
    } else {
        result
    }
}

/// Two commands ask for the same thing when their direction matches and the device's clamped
/// limit is within echo tolerance of the request.
fn same_intent(left: Command, right: Command) -> bool {
    left.direction() == right.direction()
        && left
            .watts()
            .zip(right.watts())
            .is_none_or(|(left, right)| left.abs_diff(right) <= ECHO_TOLERANCE_W)
}

/// The command for a signed setpoint; tiny values become a hold instead of a trickle charge.
fn command_for(watts: f64) -> Command {
    let magnitude = whole_watts(watts.abs());
    if magnitude == 0 {
        return Command::Hold;
    }
    if watts > 0.0 {
        Command::Charge { watts: magnitude }
    } else {
        Command::Discharge { watts: magnitude }
    }
}

/// Whole watts for the wire; non-finite and huge values are clamped instead of wrapping.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole_watts(watts: f64) -> u32 {
    if !watts.is_finite() || watts <= 0.0 {
        return 0;
    }
    watts.round().clamp(0.0, f64::from(u32::MAX)) as u32
}

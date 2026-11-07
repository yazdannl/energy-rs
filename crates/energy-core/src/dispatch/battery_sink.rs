//! The real command sink: `energy-battery`'s per-device controller, pacing and echo included.

use std::collections::BTreeMap;
use std::sync::Arc;

use energy_battery::control::{Command, DeviceController, Direction};
use energy_battery::transport::DeviceIo;

use crate::dispatch::{BoxFuture, CommandSink, SinkError, WriteReceipt};

/// A [`CommandSink`] over the device controllers.
///
/// Every write goes through the controller, so the protocol rules hold by construction: at least
/// two seconds between writes to one device, one command in flight, `smartMode: 1` on every write,
/// no `acMode` on a hold, and confirmation only when the *next report* echoes the limit. A fault
/// the controller reports (wedge, restart, echo mismatch) is returned as a [`SinkError`], which the
/// dispatcher turns into exactly one fail-safe hold and a backoff.
pub struct BatterySink<T: DeviceIo> {
    devices: BTreeMap<String, Arc<DeviceController<T>>>,
}

impl<T: DeviceIo> BatterySink<T> {
    /// A sink over the controllers, keyed by serial.
    pub fn new(devices: Vec<Arc<DeviceController<T>>>) -> Self {
        Self {
            devices: devices
                .into_iter()
                .map(|device| (device.sn().to_owned(), device))
                .collect(),
        }
    }

    /// The serials this sink may command.
    pub fn pack_ids(&self) -> Vec<&str> {
        self.devices.keys().map(String::as_str).collect()
    }

    /// Apply one command and poll for its echo.
    async fn send(&self, pack_id: String, command: Command) -> Result<WriteReceipt, SinkError> {
        let controller = self
            .devices
            .get(&pack_id)
            .ok_or_else(|| SinkError::UnknownPack {
                pack_id: pack_id.clone(),
            })?;
        let applied = controller
            .apply(command)
            .await
            .map_err(|error| SinkError::Device {
                pack_id: pack_id.clone(),
                reason: error.to_string(),
            })?;
        let observation = controller.poll().await.map_err(|error| SinkError::Device {
            pack_id: pack_id.clone(),
            reason: error.to_string(),
        })?;
        if let Some(fault) = observation.fault {
            return Err(SinkError::Fault {
                pack_id,
                fault: Box::new(fault),
            });
        }
        Ok(WriteReceipt {
            applied_w: signed_watts(applied.command),
            verified: observation.confirmed == Some(applied.command),
        })
    }
}

impl<T: DeviceIo> CommandSink for BatterySink<T> {
    fn write(&self, pack_id: &str, watts: f64) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        let command = command_for(watts);
        Box::pin(self.send(pack_id.to_owned(), command))
    }

    fn hold(&self, pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        Box::pin(self.send(pack_id.to_owned(), Command::Hold))
    }
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

/// The engine's signed watts for a command (positive charging).
fn signed_watts(command: Command) -> f64 {
    let magnitude = f64::from(command.watts().unwrap_or(0));
    if command.direction() == Some(Direction::Discharge) {
        -magnitude
    } else {
        magnitude
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

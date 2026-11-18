//! The bounded emergency hold for a wedged AC path.

use crate::control::command::Command;
use crate::control::controller::{DeviceController, MAX_EMERGENCY_HOLDS};
use crate::control::fault::ControlFault;
use crate::control::pending::Pending;
use crate::control::state::Inner;
use crate::error::BatteryError;
use crate::transport::DeviceIo;

/// While wedged only a hold may go out, and only a bounded number of times per episode.
pub(super) fn prepare_wedged<T: DeviceIo>(
    controller: &DeviceController<T>,
    inner: &mut Inner,
    command: Command,
    ac_mode: i32,
) -> Result<Pending, BatteryError> {
    let sn = controller.sn().to_owned();
    if !command.is_hold() || inner.emergency_holds >= MAX_EMERGENCY_HOLDS {
        return Err(ControlFault::Wedged { sn, ac_mode }.into());
    }
    let Some(report) = inner.last_report.clone() else {
        return Err(BatteryError::NotRead { sn });
    };
    inner.emergency_holds += 1;
    tracing::warn!(
        sn,
        ac_mode,
        attempt = inner.emergency_holds,
        max = MAX_EMERGENCY_HOLDS,
        "wedged device: sending an emergency neutralising hold"
    );
    // A hold has no magnitude, so it needs no ceiling; its write body never carries `acMode`.
    Ok(controller.install(inner, command, &report, true))
}

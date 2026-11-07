//! Wedge and restart detection on the report stream.

use crate::control::fault::ControlFault;
use crate::report::Report;

const CHARGING: i32 = 1;
const DISCHARGING: i32 = 2;
/// `messageId` counts in 8 bits; a jump of more than half the ring is a restart, not a wrap.
pub(super) const RESTART_WINDOW: u32 = 128;

/// `acMode` outside {0, 1, 2} means the write loop wedged the AC path (`127`/`251`/`255` seen in
/// the field). 0 is "not reported" and is not a fault; any other value is.
pub fn wedge(report: &Report) -> Option<ControlFault> {
    match report.properties.ac_mode {
        CHARGING | DISCHARGING | 0 => None,
        ac_mode => Some(ControlFault::Wedged {
            sn: report.sn.clone(),
            ac_mode,
        }),
    }
}

/// A device restart: the report counter went backwards across the 8-bit ring, or the device clock
/// did. Neither is proof on its own, both together are the best signal the protocol offers.
pub fn restart(previous: &Report, current: &Report) -> Option<ControlFault> {
    let delta = current.message_id.wrapping_sub(previous.message_id) & 0xFF;
    let clock_went_back = current.timestamp > 0 && current.timestamp < previous.timestamp;
    if delta > RESTART_WINDOW || clock_went_back {
        return Some(ControlFault::Restarted {
            sn: current.sn.clone(),
            before: previous.message_id,
            after: current.message_id,
        });
    }
    None
}

/// Wedge first (it forbids all writes), restart second.
pub fn inspect(previous: Option<&Report>, current: &Report) -> Option<ControlFault> {
    wedge(current).or_else(|| previous.and_then(|previous| restart(previous, current)))
}

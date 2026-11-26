//! Dispatch-row evidence for a tick whose write was suppressed by backoff.
//!
//! A suppressed tick still owes the operator a measurement. Whether a command is standing is a
//! separate question, answered only by the commands we actually issued -- not by this tick's plan.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use energy_battery::FLOW_THRESHOLD_W;

use crate::dispatch::sent::Sent;
use crate::dispatch::{AppliedPowerReason, HOLD_DEADBAND_W, Planned};

/// Commands we last issued, so backoff wording can tell a standing command from an odd flow.
pub(super) type LastCommands = BTreeMap<String, f64>;

/// Fold one attempted write into the tracked commands; a verified hold releases ownership.
pub(super) fn note_write(
    last: &mut LastCommands,
    pack_id: &str,
    command_w: f64,
    fail_safe: bool,
    verified: bool,
) {
    if (fail_safe || command_w.abs() <= HOLD_DEADBAND_W) && verified {
        last.remove(pack_id);
    } else if command_w.abs() > HOLD_DEADBAND_W {
        last.insert(pack_id.to_owned(), command_w);
    }
}

/// Track an attempted write, then explain only a write the backoff actually suppressed.
///
/// A safety bypass keeps its own receipt; a suppressed tick gets the fresh measurement instead of
/// a fabricated zero.
pub(super) fn finish(last: &Mutex<LastCommands>, plan: &Planned, sent: &mut Sent) {
    if plan.send {
        let mut tracked = last.lock().unwrap_or_else(PoisonError::into_inner);
        note_write(
            &mut tracked,
            &plan.pack_id,
            plan.command_w,
            sent.fail_safe,
            sent.verified,
        );
    }
    let Some(cause) = &plan.backoff_cause else {
        return;
    };
    if plan.send {
        return;
    }
    let (flow_w, detail) = {
        let tracked = last.lock().unwrap_or_else(PoisonError::into_inner);
        flow(plan, &tracked)
    };
    if let Some(flow_w) = flow_w {
        sent.applied_w = flow_w;
        sent.flow_measured = true;
        sent.measured_at = plan.measured_at;
        sent.applied_w_reason = None;
    } else {
        sent.applied_w_reason = Some(AppliedPowerReason::SampleStale);
    }
    sent.error = Some(format!("not sent; backing off after {cause}{detail}"));
}

/// Fresh flow and the reason text for a suppressed tick.
///
/// With no fresh read the flow is unknown, not zero, even if the ladder clamped the pack to 0 W.
/// Fresh means `available && !stale` in the pack reading, not an age inferred at this point.
pub(super) fn flow(plan: &Planned, last: &LastCommands) -> (Option<f64>, String) {
    let standing = last
        .get(&plan.pack_id)
        .is_some_and(|watts| watts.abs() > HOLD_DEADBAND_W);
    match plan.measured_flow_w {
        None => (None, "; no fresh reading, flow unknown".to_owned()),
        Some(flow_w) if flow_w.abs() <= FLOW_THRESHOLD_W as f64 => {
            let detail = if standing {
                "; previous command still standing; no flow measured"
            } else {
                "; no command is standing; no flow measured"
            };
            (Some(flow_w), detail.to_owned())
        }
        Some(flow_w) if standing => (
            Some(flow_w),
            format!("; previous command still standing; measured {flow_w:.0} W"),
        ),
        Some(flow_w) => (
            Some(flow_w),
            format!("; no command is standing; measured {flow_w:.0} W (flow unexplained)"),
        ),
    }
}

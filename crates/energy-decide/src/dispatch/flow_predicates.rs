//! Ordered predicates: first match wins, including holds and unknown status.

use crate::dispatch::constants::HOLD_DEADBAND_W;
use crate::dispatch::constants::tolerance;
use crate::dispatch::{FlowClass, FlowStatus};

/// Classify one measured receipt against its command.
pub fn classify_flow(command_w: f64, measured_w: f64, status: FlowStatus) -> FlowClass {
    use FlowClass as F;
    let magnitude = measured_w.abs();
    let flow = magnitude > HOLD_DEADBAND_W;
    let hold = command_w.abs() <= HOLD_DEADBAND_W;
    let sign = measured_w.is_sign_positive();
    let charge_status = status == FlowStatus::Charge;
    let discharge_status = status == FlowStatus::Discharge;
    let command_charge = command_w > 0.0;
    let opposite_to_command =
        (command_charge && discharge_status) || (!command_charge && charge_status);
    // S132 pins reversed status on a nonzero command as wrong_direction, so the
    // command/status comparison wins over the measured-sign/status mapping alarm.
    let mapping_alarm =
        flow && ((sign && discharge_status) || (!sign && charge_status)) && !opposite_to_command;
    if status == FlowStatus::Invalid {
        return F::StatusInconsistent;
    }
    if flow && status == FlowStatus::Idle {
        return F::IdleWithFlow;
    }
    if flow && status == FlowStatus::Unknown {
        return F::UnknownDirectionFlow;
    }
    if hold && flow && (charge_status || discharge_status) {
        return F::HoldUnexpectedFlow;
    }
    if !hold && flow && opposite_to_command {
        return F::WrongDirection;
    }
    if mapping_alarm {
        return F::StatusInconsistent;
    }
    if status == FlowStatus::Unknown {
        return F::UnknownDirectionNoFlow;
    }
    if !hold && opposite_to_command {
        return F::OppositeDirectionNoFlow;
    }
    if hold && (charge_status || discharge_status) {
        return F::HoldStatusDirectional;
    }
    if hold {
        return F::HoldVerified;
    }
    if !flow {
        return F::NoFlow;
    }
    classify_magnitude(command_w, magnitude)
}

fn classify_magnitude(command_w: f64, magnitude: f64) -> FlowClass {
    let error = (magnitude - command_w.abs()).abs();
    let tau = tolerance(command_w);
    if error <= tau {
        FlowClass::FullFlow
    } else if magnitude < command_w.abs() - tau {
        FlowClass::PartialFlow
    } else {
        FlowClass::OverFlow
    }
}

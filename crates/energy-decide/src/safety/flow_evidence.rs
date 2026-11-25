//! Shared M4 flow-evidence predicates for safety verification.

use crate::dispatch::constants::HOLD_DEADBAND_W;
use crate::dispatch::{FlowReport, FlowStatus, status_direction};

pub(crate) fn measured_neutral(report: &FlowReport) -> bool {
    report.measured_w.is_finite()
        && report.measured_w.abs() <= HOLD_DEADBAND_W
        && status_direction(report.ac_status, report.dc_status) == FlowStatus::Idle
        && report.device_clock_ok
        && report.device_clock_coherent
}

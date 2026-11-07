//! Typed control faults: the things a write can reveal that a write cannot fix.

use thiserror::Error;

use crate::control::direction::Direction;

/// A control-plane fault that must stop or reset the commanding, never be retried blindly.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ControlFault {
    #[error(
        "device {sn} reports acMode {ac_mode}: the AC path is wedged, stop writing and power-cycle it"
    )]
    Wedged { sn: String, ac_mode: i32 },
    #[error(
        "device {sn} restarted between reports (messageId {before} -> {after}); re-read before commanding"
    )]
    Restarted { sn: String, before: u32, after: u32 },
    #[error(
        "device {sn} is in acMode {ac_mode}, expected {expected}: the direction change did not take"
    )]
    WrongMode {
        sn: String,
        expected: Direction,
        ac_mode: i32,
    },
    #[error("write to {sn} not confirmed: {direction} {expected_w} W, report echoes {echoed_w} W")]
    EchoMismatch {
        sn: String,
        direction: Direction,
        expected_w: u32,
        echoed_w: u32,
    },
    #[error(
        "report for {got} arrived on the {expected} controller: it cannot confirm or fault a write"
    )]
    ForeignReport { expected: String, got: String },
    #[error(
        "hold on {sn} not confirmed: report echoes inputLimit {input_w} W / outputLimit {output_w} W"
    )]
    HoldEcho {
        sn: String,
        input_w: u32,
        output_w: u32,
    },
}

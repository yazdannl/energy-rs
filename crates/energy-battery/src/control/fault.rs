//! Typed control faults: the things a write can reveal that a write cannot fix.

use thiserror::Error;

use crate::control::direction::Direction;

/// A control-plane fault that must stop or reset the commanding, never be retried blindly.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ControlFault {
    #[error(
        "uncertain_write: device {sn} request {request_id} timed out ({detail}); the device may have accepted it, send and verify a hold before writing again"
    )]
    UncertainWrite {
        sn: String,
        request_id: u32,
        detail: String,
    },
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
    #[error("write to {sn} (request {request_id}) not confirmed within {window_ms} ms: {detail}")]
    ConfirmationTimeout {
        sn: String,
        request_id: u32,
        window_ms: u64,
        detail: String,
    },
    #[error(
        "write to {sn} echoed {direction} {echoed_w} W but measured only {observed_w} W: the AC path did not carry power"
    )]
    NoFlow {
        sn: String,
        direction: Direction,
        echoed_w: u32,
        observed_w: i64,
        /// Device clock of the sample that judged this command.
        sample_at: i64,
        /// Epoch seconds of the pre-write report that bounds the command.
        write_at: i64,
    },
    #[error(
        "hold on {sn} not confirmed: report echoes inputLimit {input_w} W / outputLimit {output_w} W"
    )]
    HoldEcho {
        sn: String,
        input_w: u32,
        output_w: u32,
    },
}

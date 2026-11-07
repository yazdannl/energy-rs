//! The one error type the crate returns, plus the control fault it wraps.

use thiserror::Error;

use crate::control::ControlFault;

/// Everything a zenSDK client can refuse.
///
/// The variants are distinct where the caller must behave differently: an HTTP 401 means the local
/// API is switched off (enable HEMS in the app), a body code 401 means the configured serial is
/// wrong, and 400/413 mean do not retry blindly.
#[derive(Debug, Error)]
pub enum BatteryError {
    #[error("device {host}:{port} request failed: {reason}")]
    Transport {
        host: String,
        port: u16,
        reason: String,
    },
    #[error(
        "local API disabled on {host}: enable HEMS in the Zendure app, open it once, then retry"
    )]
    Disabled { host: String },
    #[error("device {host}:{port} answered HTTP {status}")]
    HttpStatus {
        host: String,
        port: u16,
        status: u16,
    },
    #[error("device rejected the write with code {code}: {message}")]
    Rejected { code: i32, message: String },
    #[error("serial mismatch: sent {sent}, device reports {got}")]
    SnMismatch { sent: String, got: String },
    #[error("write body is {len} bytes, the device accepts at most {limit}")]
    BodyTooLarge { len: usize, limit: usize },
    #[error("device reports a receive-buffer overflow (code 413): {detail}")]
    BufferOverflow { detail: String },
    #[error("payload is not usable: {reason}")]
    Payload { reason: String },
    #[error("discovery failed: {reason}")]
    Discovery { reason: String },
    #[error("{reason}")]
    Invalid { reason: String },
    #[error("device {sn} was never read: poll a report before commanding it")]
    NotRead { sn: String },
    #[error(transparent)]
    Fault(#[from] ControlFault),
}

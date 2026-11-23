//! Transport acknowledgement with the instant it actually arrived.

use std::time::Instant;

use crate::dispatch::WriteAck;

/// The future's own return value; no channel is involved.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteResult {
    /// Transport acknowledgement.
    pub ack: WriteAck,
    /// Instant the transport call returned.
    pub completed_at: Instant,
}

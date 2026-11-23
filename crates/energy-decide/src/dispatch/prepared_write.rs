//! A command that passed both consumer ports and owns an open transaction.

use crate::dispatch::{PackCommand, SlotRequest};

/// Write ready to start on a worker thread.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedWrite {
    /// Command to send.
    pub command: PackCommand,
    /// Transaction id held by the coordinator until the budget ends.
    pub tx_id: u64,
    /// Safety reason recorded when M5 replaced the economic target.
    pub gate_reason: Option<&'static str>,
    /// Original economic request, kept for an honest row.
    pub request: SlotRequest,
    /// True when a stale partial pause was cancelled by a changed target.
    pub interrupted: bool,
}

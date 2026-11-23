//! Effects of a transport attempt that the coordinator must apply to state.

use crate::dispatch::{DispatchRow, PendingConfirmation};

/// Row and state changes produced by one bounded write.
#[derive(Debug, Clone, PartialEq)]
pub struct AttemptEffect {
    /// Observable row.
    pub row: DispatchRow,
    /// Deferred confirmation to store, if the transport accepted.
    pub pending: Option<PendingConfirmation>,
    /// Typed reason to mark controller uncertainty, if any.
    pub uncertain_reason: Option<&'static str>,
    /// Whether an ordinary fault backoff should be armed.
    pub arm_backoff: bool,
}

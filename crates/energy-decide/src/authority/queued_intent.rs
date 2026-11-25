//! Queued work is an intent, never an authorisation.

use crate::authority::AuthorityState;
use crate::dispatch::PackCommand;

/// A device command queued earlier; its enqueue-time state grants nothing.
#[derive(Debug, Clone)]
pub struct QueuedIntent {
    /// Stable intent id preserved in the refusal row.
    pub id: u64,
    /// Device command to attempt at execution time.
    pub command: PackCommand,
    /// Instant the intent was queued.
    pub queued_at_ms: i64,
    /// Authority state at enqueue time, for audit correlation only.
    pub queued_state: AuthorityState,
}

impl QueuedIntent {
    /// Queue an intent; no gate call and no device effect happens here.
    pub fn new(id: u64, command: PackCommand, queued_at_ms: i64, state: AuthorityState) -> Self {
        Self {
            id,
            command,
            queued_at_ms,
            queued_state: state,
        }
    }
}

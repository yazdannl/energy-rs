//! Immutable terminal states for one write future.

use crate::dispatch::WriteResult;

/// The single terminal decision for a write; once set it never changes.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteTerminal {
    /// No terminal state has been published yet.
    Pending,
    /// Transport returned; the recorded instant decides on-time vs late.
    Completed(WriteResult),
    /// The future's absolute deadline passed with no recorded completion.
    Expired,
}

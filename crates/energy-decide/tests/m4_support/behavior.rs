//! Transport behaviour for one fake pack.

/// Transport behaviour for one pack.
#[derive(Debug, Clone, PartialEq)]
pub enum Behavior {
    /// Accept with or without a device echo.
    Accept { echo: bool },
    /// Explicit device refusal.
    Reject(&'static str),
    /// Transport error before acceptance.
    Fail(&'static str),
    /// Write stays in flight until cancelled or `max_wait_ms` elapses.
    Stuck { max_wait_ms: u64 },
    /// Never-returning transport in the original incident; cancellable here.
    Never,
    /// Completion ignores cancellation and arrives after the deadline.
    Late { after_ms: u64 },
    /// Fail the first N writes, then accept with an echo.
    FailFirst(usize, &'static str),
}

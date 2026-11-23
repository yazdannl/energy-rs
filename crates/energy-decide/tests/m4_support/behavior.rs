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
    /// Transport call never returns.
    Never,
    /// Fail the first N writes, then accept with an echo.
    FailFirst(usize, &'static str),
}

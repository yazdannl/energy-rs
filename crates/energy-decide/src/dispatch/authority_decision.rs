//! Answer from the provisional M6 consumer port.

/// Authority answer for one device command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityDecision {
    /// Execution-time authority granted by M6.
    Authorized,
    /// Authority declined; M4 records and never retries around it.
    Denied {
        /// Typed reason supplied by M6.
        reason: &'static str,
    },
}

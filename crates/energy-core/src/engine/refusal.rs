//! The last refused plan solve, kept so the UI can say why it is holding.

/// Why the last solve was refused, and when it was last seen.
///
/// A refusal is a hold, not an error the loop stops on: the engine keeps ticking, keeps holding and
/// keeps trying. Without this record the reason only exists in the ledger and the add-on log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The typed refusal, already formatted (never a token or a file path).
    pub reason: String,
    /// When the refusal was last seen, UTC epoch seconds.
    pub at: i64,
}

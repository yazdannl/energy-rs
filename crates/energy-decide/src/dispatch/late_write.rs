//! A transport completion that arrived after its write generation was superseded.

/// Late completion from a write whose deadline already fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LateWrite {
    /// Pack whose write completed late.
    pub pack_id: String,
    /// Monotonic generation of the superseded write.
    pub generation: u64,
    /// Typed reason; always `late_after_deadline_superseded`.
    pub reason: &'static str,
}

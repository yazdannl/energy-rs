//! The driver exceeded its per-tick poll bound; no future is restarted.

/// Recorded when one tick's arbiter loop exceeds its allowed poll count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveBoundEvent {
    /// Pack whose future was still unresolved.
    pub pack_id: String,
    /// Monotonic generation of the write.
    pub generation: u64,
    /// Polls used before the bound fired.
    pub polls: u64,
}

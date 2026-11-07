//! Wiring: keep the ledger's measured, learned and protected state current from the live fleet.
//!
//! The engine's tick calls these in order after the fleet read:
//!
//! * [`packs`] — register a pack on first sight and remember its ledger id.
//! * [`measurements`] — one 1-minute sample per pack plus the 5-minute rollup.
//! * [`sessions`] — re-extract completed runs and relearn the pack that just finished one.
//! * [`cells`] — evaluate the low-cell latch from the newest `min_cell_v` and persist changes.
//! * [`maintenance`] — consult the weekly full-charge scheduler before the plan and turn its answer
//!   into expiring per-pack overrides.
//!
//! Saving (P9) is deliberately absent: savings rows are derived from measured integrals and belong
//! to the savings phase, not to the control loop.

pub(crate) mod cells;
pub(crate) mod error;
pub(crate) mod maintenance;
pub(crate) mod measurements;
pub(crate) mod packs;
pub(crate) mod sessions;

pub mod latch_store;
pub mod learning_store;

pub use error::WiringError;
pub use latch_store::LedgerLatchStore;
pub use learning_store::LedgerLearningStore;

/// Whole watts as the zenSDK wire format wants them; non-finite and negative values become 0.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn watts_u32(watts: f64) -> u32 {
    if !watts.is_finite() || watts <= 0.0 {
        return 0;
    }
    watts.round().clamp(0.0, f64::from(u32::MAX)) as u32
}

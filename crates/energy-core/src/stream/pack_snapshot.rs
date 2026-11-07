//! One pack's live state, learned profile, protection latch and manual override.

use energy_battery::{CellLatch, LearningProfile};

use crate::dispatch::ManualOverride;

/// Everything the Batteries screen and the safety ladder need to know about one pack.
///
/// The identity (serial, name, nameplate) stays in the ledger; this is the live part. `profile` and
/// `latch` are the durable learning/protection values the engine last used, carried as their own
/// types so the learner's provenance slugs survive untouched.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PackSnapshot {
    /// Pack serial number (the stable key).
    pub sn: String,
    /// Reported state of charge, percent.
    pub soc_pct: Option<f64>,
    /// Signed AC power, W; positive charges the pack.
    pub power_w: Option<f64>,
    /// When the reading was taken, UTC epoch seconds.
    pub sampled_at: Option<i64>,
    /// Whether the engine considers the reading stale.
    pub stale: bool,
    /// Effective charge limit after config and device clamping, W.
    pub max_charge_w: Option<f64>,
    /// Effective discharge limit after config and device clamping, W.
    pub max_discharge_w: Option<f64>,
    /// The last applied learning profile (capacity/efficiency and confidence).
    pub profile: Option<LearningProfile>,
    /// The persisted low-cell latch.
    pub latch: Option<CellLatch>,
    /// A manual override while it is still in force.
    pub override_state: Option<ManualOverride>,
}

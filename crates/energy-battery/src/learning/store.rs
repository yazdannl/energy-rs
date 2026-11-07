//! Persistence for learning profiles.

use crate::error::BatteryError;
use crate::learning::profile::LearningProfile;

/// Where a pack's learning profile is stored.
///
/// The learner is pure: the engine owns the database (or the ledger), so it hands the learner a
/// store through this trait. A failed save is returned to the caller, never swallowed — a profile
/// that cannot be persisted will be relearned from scratch, and that has to be visible.
pub trait LearningStore: Send + Sync {
    /// The last saved profile for `pack_id`, or `None` when the pack was never learned.
    fn load(&self, pack_id: &str) -> Result<Option<LearningProfile>, BatteryError>;

    /// Replace `pack_id`'s profile.
    fn save(&self, profile: &LearningProfile) -> Result<(), BatteryError>;
}

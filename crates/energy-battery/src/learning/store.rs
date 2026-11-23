//! Persistence for learning profiles and their pending audit events.

use crate::error::BatteryError;
use crate::learning::event::LearningEvent;
use crate::learning::profile::LearningProfile;

/// Where a pack's learning profile is stored.
///
/// The learner itself is pure: the engine owns the database (or the ledger), so it hands the learner a
/// store through this trait. A failed save is returned to the caller, never swallowed — a profile
/// that cannot be persisted will be relearned from scratch, and that has to be visible.
pub trait LearningStore: Send + Sync {
    /// The last saved profile for `pack_id`, or `None` when the pack was never learned.
    fn load(&self, pack_id: &str) -> Result<Option<LearningProfile>, BatteryError>;

    /// Replace `pack_id`'s profile.
    fn save(&self, profile: &LearningProfile) -> Result<(), BatteryError>;

    /// Persist a profile and its pending audit event together.
    ///
    /// Stores that own a durable outbox override this and return the committed event with its id.
    /// The default keeps simple test stores working: they still persist the profile, and the event
    /// is returned uncommitted.
    fn save_with_event(
        &self,
        profile: &LearningProfile,
        event: LearningEvent,
    ) -> Result<LearningEvent, BatteryError> {
        self.save(profile)?;
        Ok(event)
    }

    /// Not-yet-journalled audit events from earlier passes, oldest first.
    fn pending_events(&self) -> Result<Vec<LearningEvent>, BatteryError> {
        Ok(Vec::new())
    }

    /// Write one audit event and clear its outbox row only after the write succeeded.
    fn publish_event(&self, _event: &LearningEvent) -> Result<(), BatteryError> {
        Ok(())
    }
}

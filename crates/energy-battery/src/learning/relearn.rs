//! Load, learn, persist: one pack's learning pass.

use chrono::{DateTime, Utc};

use crate::error::BatteryError;
use crate::learning::bounds::CapacityBounds;
use crate::learning::profile::LearningProfile;
use crate::learning::report::LearningReport;
use crate::learning::session::SessionRecord;
use crate::learning::store::LearningStore;

/// Relearn one pack from its sessions and persist the resulting profile.
///
/// The previous profile is loaded first so thin data can keep it instead of resetting to the typed
/// defaults. The save happens even when nothing was applied: the profile records the refusal
/// (`capacity_decision`, `sessions_used`), which is what makes an empty learner debuggable.
pub fn relearn(
    store: &dyn LearningStore,
    pack_id: &str,
    sessions: &[SessionRecord],
    bounds: &CapacityBounds,
    now: DateTime<Utc>,
) -> Result<LearningReport, BatteryError> {
    let previous = store.load(pack_id)?;
    let report = LearningReport::learn(pack_id, sessions, bounds, previous.as_ref(), now);
    store.save(&LearningProfile::from_report(&report))?;
    Ok(report)
}

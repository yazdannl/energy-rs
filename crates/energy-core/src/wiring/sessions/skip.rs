//! Persistence for a scheduled check that deliberately did not learn.

use energy_battery::learning::{
    CapacityBounds, LearningProfile, LearningReport, LearningStore, NEVER_MEASURED_AT,
    PairDiagnostics,
};

use crate::engine::instant;
use crate::wiring::error::WiringError;

/// Why one scheduled check did not learn, and what it saw.
#[derive(Debug, Clone, Copy)]
pub(super) struct SkippedPass<'a> {
    pub(super) sn: &'a str,
    pub(super) now: i64,
    pub(super) sessions_in_window: usize,
    pub(super) sessions_complete: usize,
    pub(super) blocker: &'static str,
}

pub(super) fn note_skipped(
    store: &dyn LearningStore,
    previous: Option<LearningProfile>,
    bounds: &CapacityBounds,
    skipped: SkippedPass<'_>,
) -> Result<(), WiringError> {
    let mut profile = previous.unwrap_or_else(|| {
        // A diagnostic row is not a measurement: its `updatedAt` stays at the never-measured
        // sentinel until a real cycle publishes a number.
        let report = LearningReport::learn(skipped.sn, &[], bounds, None, instant(skipped.now));
        let mut profile = LearningProfile::from_report(&report);
        profile.updated_at = NEVER_MEASURED_AT;
        profile.efficiency_diagnostics.last_learning_at = None;
        profile
    });
    let last_learning_at = profile.efficiency_diagnostics.last_learning_at;
    profile.efficiency_diagnostics = PairDiagnostics::skipped(
        instant(skipped.now),
        skipped.sessions_in_window,
        skipped.sessions_complete,
        skipped.blocker,
    );
    profile.efficiency_diagnostics.last_learning_at = last_learning_at;
    store.save(&profile)?;
    Ok(())
}

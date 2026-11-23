//! Bounds for re-extraction: revisit unfinished runs without re-reading month-old history.

use energy_battery::learning::LearningProfile;
use energy_ledger::Session;

use super::SessionPolicy;

/// How far back this pass must re-extract.
///
/// Sessions are rewritten only when their *start* lies in the extraction window, so a run that was
/// open when first scanned has to be revisited from its start once its trailing hold arrives.
/// Every unresolved run inside the learning horizon is revisited, not only one with a recent end:
/// a closing sample can land long after a gap. Crossings are resolved against the **final** floor,
/// after the horizon clamp and the open/gap adjustments, so re-extraction never leaves a duplicate
/// tail. A cold ledger gets one bounded backfill, and a gap since the last *learning* pass forces a
/// backfill even when skips kept refreshing the attempt.
pub(super) fn extraction_floor(
    policy: &SessionPolicy,
    now: i64,
    from: i64,
    stored: &[Session],
    previous: Option<&LearningProfile>,
) -> i64 {
    let recent = now.saturating_sub(policy.extract_seconds());
    // Profiles written before attempt timestamps may hold runs left incomplete by the old
    // one-hour extraction bug. Repair the whole window once, then resume incremental passes.
    if previous.is_some_and(|profile| profile.efficiency_diagnostics.last_attempt_at.is_none()) {
        return from;
    }
    if stored.is_empty() {
        return from.max(now.saturating_sub(48 * 3600));
    }
    let open = stored
        .iter()
        .filter(|session| !session.complete && session.invalidated_reason.is_none())
        .map(|session| session.started_at.saturating_sub(60))
        .min();
    let gap = previous
        .and_then(|profile| profile.efficiency_diagnostics.last_learning_at)
        .filter(|at| now.saturating_sub(at.timestamp()) > policy.extract_seconds())
        .map(|at| at.timestamp().saturating_sub(60));
    let mut floor = [Some(recent), open, gap]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(recent)
        .max(from);
    // Resolve crossings against the final boundary. DELETE matches only starts inside the window,
    // so a run that starts before this floor and ends after it would survive and extraction would
    // insert its tail as a second session. Lower the floor to that run's start and repeat: the new
    // floor can expose an older overlapping run. Stored rows all start inside the horizon, so the
    // clamp still holds and each step strictly lowers the floor.
    while let Some(start) = stored
        .iter()
        .filter(|session| session.started_at < floor && session.ended_at > floor)
        .map(|session| session.started_at.max(from))
        .min()
    {
        if start >= floor {
            break;
        }
        floor = start;
    }
    floor
}

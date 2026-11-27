//! Session re-extraction and capacity relearning after a completed run.

use energy_battery::learning::{
    CapacityBounds, CapacityPool, LearningEvent, LearningProfile, LearningReport, LearningStore,
    NEVER_MEASURED_AT,
};
use energy_ledger::{Ledger, Session, SessionRules, Tier};

use crate::engine::instant;
use crate::wiring::error::WiringError;

/// How often sessions are re-extracted and how far back learning reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SessionPolicy {
    check_seconds: i64,
    window_hours: i64,
    rules: SessionRules,
}

impl SessionPolicy {
    /// The shipped policy: extract every `check_seconds`, learn from `window_hours` of history.
    pub(crate) fn new(check_seconds: i64, window_hours: i64) -> Self {
        Self {
            check_seconds: check_seconds.max(60),
            window_hours: window_hours.max(1),
            rules: SessionRules::default(),
        }
    }

    /// The extraction cadence, seconds.
    pub(crate) fn check_seconds(&self) -> i64 {
        self.check_seconds
    }

    fn window_seconds(&self) -> i64 {
        self.window_hours.saturating_mul(3600)
    }

    /// The extraction window: at least an hour, so a run that started before the check is whole.
    fn extract_seconds(&self) -> i64 {
        self.check_seconds.saturating_mul(4).max(3600)
    }
}

/// Whether the session check is due.
pub(crate) fn due(now: i64, last: Option<i64>, interval: i64) -> bool {
    last.is_none_or(|at| now.saturating_sub(at) >= interval)
}

/// Re-extract and relearn every pack with a completed session newer than its last measurement.
///
/// The trigger is a *new* episode, not a wall-clock coincidence: a cycle that finished while the
/// engine was down is still news after a 24-hour gap. Packs with nothing new get their diagnostics
/// updated (attempt time, window counts and a named blocker) without moving `updatedAt`.
pub(crate) fn run(
    ledger: &Ledger,
    store: &dyn LearningStore,
    now: i64,
    policy: &SessionPolicy,
) -> Result<Vec<LearningReport>, WiringError> {
    scan(ledger, store, now, policy, None, true)
}

/// Relearn now, for one pack or the fleet, whatever the completion trigger says.
pub(crate) fn relearn_now(
    ledger: &Ledger,
    store: &dyn LearningStore,
    now: i64,
    policy: &SessionPolicy,
    only: Option<&str>,
) -> Result<Vec<LearningReport>, WiringError> {
    scan(ledger, store, now, policy, only, false)
}

/// The shared pass: extract, pool identical labels, then learn the selected packs.
///
/// The pool is built from *every* pack's current window, including packs that are skipped or
/// excluded by a targeted relearn. Pool membership is an evidence property, not a scheduling one.
fn scan(
    ledger: &Ledger,
    store: &dyn LearningStore,
    now: i64,
    policy: &SessionPolicy,
    only: Option<&str>,
    completed_only: bool,
) -> Result<Vec<LearningReport>, WiringError> {
    // Replay before touching profiles: an event that was committed with an earlier profile is
    // still owed to the journal even if this pass later skips that pack or fails on another one.
    for event in store.pending_events()? {
        store.publish_event(&event)?;
    }
    let states = collect_states(ledger, store, now, policy)?;
    let pool = pool::fleet_pool(&states, now);
    let mut reports = Vec::new();
    for state in states {
        if only.is_some_and(|sn| sn != state.sn) {
            continue;
        }
        if completed_only
            && let Some(blocker) = skip_reason(
                &state.sessions,
                state.complete,
                state.previous.as_ref(),
                now,
                policy,
            )
        {
            let skipped = skip::SkippedPass {
                sn: &state.sn,
                now,
                sessions_in_window: state.sessions.len(),
                sessions_complete: state.complete,
                blocker,
            };
            skip::note_skipped(store, state.previous, &state.bounds, skipped)?;
            continue;
        }
        if state.records.is_empty() {
            continue;
        }
        reports.push(learn_and_publish(ledger, store, &state, &pool, now)?);
    }
    Ok(reports)
}

/// Extract every pack before any skip or target filter is applied.
fn collect_states(
    ledger: &Ledger,
    store: &dyn LearningStore,
    now: i64,
    policy: &SessionPolicy,
) -> Result<Vec<pool::PackState>, WiringError> {
    let packs = ledger.packs()?.all()?;
    let mut states = Vec::with_capacity(packs.len());
    for pack in &packs {
        let from = now.saturating_sub(policy.window_seconds());
        let to = now.saturating_add(1);
        let previous = store.load(&pack.sn)?;
        let stored = ledger.sessions()?.for_pack(pack.id, from, to)?;
        let extract_from = floor::extraction_floor(policy, now, from, &stored, previous.as_ref());
        ledger
            .sessions()?
            .extract(pack.id, Tier::OneMinute, extract_from, to, &policy.rules)?;
        let sessions = ledger.sessions()?.for_pack(pack.id, from, to)?;
        let complete = sessions.iter().filter(|session| usable(session)).count();
        let records = sessions
            .iter()
            .map(|session| record::map(session, &pack.sn))
            .collect();
        let bounds = CapacityBounds::new(pack.label_capacity_kwh, pack.reported_capacity_kwh);
        states.push(pool::PackState {
            sn: pack.sn.clone(),
            id: pack.id,
            bounds,
            sessions,
            records,
            complete,
            previous,
        });
    }
    Ok(states)
}

/// Publish the pack row, commit the profile with its pending audit row, then journal the event.
///
/// The profile-plus-outbox commit is the commit point: its clocks decide whether a pack needs
/// another pass, and its outbox row survives a failed journal write. If the row write fails, nothing
/// has advanced; if the profile write fails, the row is already correct but the old profile still
/// grants an immediate retry. Both writes are idempotent, so the retry simply rewrites the same row
/// and then commits the profile again.
fn learn_and_publish(
    ledger: &Ledger,
    store: &dyn LearningStore,
    state: &pool::PackState,
    pool: &CapacityPool,
    now: i64,
) -> Result<LearningReport, WiringError> {
    let report = LearningReport::learn_with_pool(
        &state.sn,
        &state.records,
        &state.bounds,
        state.previous.as_ref(),
        Some(pool),
        instant(now),
    );
    let profile = LearningProfile::from_report(&report);
    let event = LearningEvent::new(state.sn.clone(), now, report.summary());
    // `learned_at` is also the provenance stamp the precedence resolver reads. Only a confident
    // measurement or a confident profile carried forward publishes a round trip: capacity-only
    // and partial-pair passes keep the typed seed, so they keep the never-measured stamp and an
    // operator's entry stays the default.
    let published_round_trip = report.efficiency.applied || report.efficiency.kept_previous;
    let learned_at = if published_round_trip {
        report.measured_at.timestamp()
    } else {
        NEVER_MEASURED_AT.timestamp()
    };
    store_row(ledger, state.id, &report, learned_at)?;
    let event = store.save_with_event(&profile, event)?;
    store.publish_event(&event)?;
    Ok(report)
}

fn usable(session: &Session) -> bool {
    session.complete
        && session.invalidated_reason.is_none()
        && session.soc_start.is_some()
        && session.soc_end.is_some()
}

/// A pass that has nothing new to learn still happened: say why, and when.
fn skip_reason(
    sessions: &[Session],
    complete: usize,
    previous: Option<&LearningProfile>,
    now: i64,
    policy: &SessionPolicy,
) -> Option<&'static str> {
    // A brand-new pack with no evidence still gets a durable, named diagnostic; otherwise the
    // packs endpoint would show nothing and the operator could not tell it apart from a dead
    // learner. A new pack that does have sessions still gets its first pass.
    if previous.is_none() && sessions.is_empty() {
        return Some("no_sessions_in_window");
    }
    let fresh = previous.is_none_or(|profile| {
        let learned_at = profile.updated_at.timestamp();
        sessions
            .iter()
            .any(|session| usable(session) && session.ended_at > learned_at)
    });
    if fresh {
        return None;
    }
    if sessions.is_empty() {
        return Some("no_sessions_in_window");
    }
    if complete == 0 {
        return Some("no_completed_session");
    }
    // Nothing new is a reason to skip only while the last real learning pass is still fresh.
    // Diagnostic skips must not refresh this clock, or a five-minute cadence could never build a
    // ten-minute gap and the backfill would be postponed forever.
    let gap = policy.check_seconds().saturating_mul(2).max(3600);
    let recent = previous
        .and_then(|profile| profile.efficiency_diagnostics.last_learning_at)
        .is_some_and(|at| now.saturating_sub(at.timestamp()) < gap);
    recent.then_some("no_new_completed_session")
}

/// Publish the learned numbers on the pack row, so `/api/v1/packs` and the planner agree.
fn store_row(
    ledger: &Ledger,
    id: i64,
    report: &LearningReport,
    now: i64,
) -> Result<(), WiringError> {
    // The pack row has one confidence column. It describes the learned capacity: an unmeasured
    // round trip (confidence 0 by construction) must not report a learned capacity as untrusted.
    // The efficiency's own confidence travels in the learning profile and the packs endpoint.
    let confidence = report.capacity.confidence;
    ledger.packs()?.set_learning(
        id,
        report.capacity.applied_kwh,
        report.efficiency.round_trip,
        confidence,
        now,
    )?;
    Ok(())
}

mod floor;
mod pool;
mod record;
mod skip;

#[cfg(test)]
mod attempt_tests;
#[cfg(test)]
mod boundary_tests;
#[cfg(test)]
mod event_tests;
#[cfg(test)]
mod failure_tests;
#[cfg(test)]
mod legacy_eta_tests;
#[cfg(test)]
mod legacy_tests;
#[cfg(test)]
mod manual_eta_tests;
#[cfg(test)]
mod pooling_tests;
#[cfg(test)]
mod scheduled_tests;
#[cfg(test)]
mod sessions_tests;

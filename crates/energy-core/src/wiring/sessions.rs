//! Session re-extraction and capacity relearning after a completed run.

use energy_battery::control::Direction as BatteryDirection;
use energy_battery::learning::{
    CapacityBounds, CapacityPool, LearningProfile, LearningReport, LearningStore, SessionRecord,
    relearn_with_pool,
};
use energy_ledger::session::Direction as LedgerDirection;
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
fn scan(
    ledger: &Ledger,
    store: &dyn LearningStore,
    now: i64,
    policy: &SessionPolicy,
    only: Option<&str>,
    completed_only: bool,
) -> Result<Vec<LearningReport>, WiringError> {
    let inputs = collect_inputs(ledger, store, now, policy, only, completed_only)?;
    let peers: Vec<(f64, &[SessionRecord])> = inputs
        .iter()
        .map(|input| (input.2.label_kwh, input.3.as_slice()))
        .collect();
    let pool = CapacityPool::from_sessions(&peers, instant(now));
    let mut reports = Vec::with_capacity(inputs.len());
    for (sn, id, bounds, records) in inputs {
        let report = relearn_with_pool(store, &sn, &records, &bounds, Some(&pool), instant(now))?;
        store_row(ledger, id, &report, report.measured_at.timestamp())?;
        reports.push(report);
    }
    Ok(reports)
}

/// One pack's inputs: serial, row id, bounds and the window's session records.
type PackInput = (String, i64, CapacityBounds, Vec<SessionRecord>);

/// Extract and load the window for every selected pack before any pack is learned.
fn collect_inputs(
    ledger: &Ledger,
    store: &dyn LearningStore,
    now: i64,
    policy: &SessionPolicy,
    only: Option<&str>,
    completed_only: bool,
) -> Result<Vec<PackInput>, WiringError> {
    let packs = ledger.packs()?.all()?;
    let mut inputs = Vec::new();
    for pack in &packs {
        if only.is_some_and(|sn| sn != pack.sn) {
            continue;
        }
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
        if completed_only
            && let Some(blocker) = skip_reason(&sessions, complete, previous.as_ref(), now, policy)
        {
            let bounds = CapacityBounds::new(pack.label_capacity_kwh, pack.reported_capacity_kwh);
            let skipped = skip::SkippedPass {
                sn: &pack.sn,
                now,
                sessions_in_window: sessions.len(),
                sessions_complete: complete,
                blocker,
            };
            skip::note_skipped(store, previous, &bounds, skipped)?;
            continue;
        }
        let records: Vec<SessionRecord> = sessions
            .iter()
            .map(|session| record(session, &pack.sn))
            .collect();
        if records.is_empty() {
            continue;
        }
        let bounds = CapacityBounds::new(pack.label_capacity_kwh, pack.reported_capacity_kwh);
        inputs.push((pack.sn.clone(), pack.id, bounds, records));
    }
    Ok(inputs)
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

/// Map one ledger session into the learner's plain record.
///
/// Invalid runs are *kept* as records with their direction, time bounds and a refusal marker.
/// Dropping them upstream would erase a direction change: a discarded discharge between two
/// charges would let the learner merge the charges and book stored energy as a loss.
fn record(session: &Session, pack_sn: &str) -> SessionRecord {
    SessionRecord {
        pack_id: pack_sn.to_owned(),
        direction: match session.direction {
            LedgerDirection::Charge => BatteryDirection::Charge,
            LedgerDirection::Discharge => BatteryDirection::Discharge,
        },
        started_at: instant(session.started_at),
        ended_at: instant(session.ended_at),
        soc_start_pct: session.soc_start.unwrap_or(f64::NAN),
        soc_end_pct: session.soc_end.unwrap_or(f64::NAN),
        energy_ac_kwh: session.energy_ac_kwh,
        duration_s: session.duration_s as f64,
        sample_count: u32::try_from(session.sample_count).unwrap_or(u32::MAX),
        complete: session.complete && session.invalidated_reason.is_none(),
    }
}

mod floor;
mod skip;

#[cfg(test)]
mod attempt_tests;
#[cfg(test)]
mod boundary_tests;
#[cfg(test)]
mod legacy_tests;
#[cfg(test)]
mod scheduled_tests;
#[cfg(test)]
mod sessions_tests;

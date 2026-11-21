//! Session re-extraction and capacity relearning after a completed run.

use energy_battery::control::Direction as BatteryDirection;
use energy_battery::learning::{
    CapacityBounds, CapacityPool, LearningReport, LearningStore, SessionRecord, relearn_with_pool,
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

/// Re-extract the recent window for every pack and relearn any pack whose run just completed.
///
/// A pack is relearned when a completed session ended inside the last two check intervals: that is
/// "a session completed", which is the trigger the learning policy asks for. The pass itself reads
/// the long window, so one new completion updates the estimate from all of the pack's history.
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
    let inputs = collect_inputs(ledger, policy, now, only, completed_only)?;
    let peers: Vec<(f64, &[SessionRecord])> = inputs
        .iter()
        .map(|input| (input.2.label_kwh, input.3.as_slice()))
        .collect();
    let pool = CapacityPool::from_sessions(&peers, instant(now));
    let mut reports = Vec::with_capacity(inputs.len());
    for (sn, id, bounds, records) in inputs {
        let report = relearn_with_pool(store, &sn, &records, &bounds, Some(&pool), instant(now))?;
        store_row(ledger, id, &report, now)?;
        reports.push(report);
    }
    Ok(reports)
}

/// One pack's inputs: serial, row id, bounds and the window's session records.
type PackInput = (String, i64, CapacityBounds, Vec<SessionRecord>);

/// Extract and load the window for every selected pack before any pack is learned.
fn collect_inputs(
    ledger: &Ledger,
    policy: &SessionPolicy,
    now: i64,
    only: Option<&str>,
    completed_only: bool,
) -> Result<Vec<PackInput>, WiringError> {
    let packs = ledger.packs()?.all()?;
    let mut inputs = Vec::new();
    for pack in &packs {
        if only.is_some_and(|sn| sn != pack.sn) {
            continue;
        }
        let extract_from = now.saturating_sub(policy.extract_seconds());
        ledger.sessions()?.extract(
            pack.id,
            Tier::OneMinute,
            extract_from,
            now.saturating_add(1),
            &policy.rules,
        )?;
        let from = now.saturating_sub(policy.window_seconds());
        let sessions = ledger
            .sessions()?
            .for_pack(pack.id, from, now.saturating_add(1))?;
        if completed_only {
            let recent = policy.check_seconds().saturating_mul(2);
            let just_finished = sessions
                .iter()
                .any(|session| session.complete && session.ended_at >= now.saturating_sub(recent));
            if !just_finished {
                continue;
            }
        }
        let records: Vec<SessionRecord> = sessions
            .iter()
            .filter_map(|session| record(session, &pack.sn))
            .collect();
        if records.is_empty() {
            continue;
        }
        let bounds = CapacityBounds::new(pack.label_capacity_kwh, pack.reported_capacity_kwh);
        inputs.push((pack.sn.clone(), pack.id, bounds, records));
    }
    Ok(inputs)
}

/// Publish the learned numbers on the pack row, so `/api/v1/packs` and the planner agree.
fn store_row(
    ledger: &Ledger,
    id: i64,
    report: &LearningReport,
    now: i64,
) -> Result<(), WiringError> {
    let confidence = report.capacity.confidence.min(report.efficiency.confidence);
    ledger.packs()?.set_learning(
        id,
        report.capacity.applied_kwh,
        report.efficiency.round_trip,
        confidence,
        now,
    )?;
    Ok(())
}

/// Map one ledger session into the learner's plain record; incomplete or SoC-less runs are skipped.
fn record(session: &Session, pack_sn: &str) -> Option<SessionRecord> {
    let (Some(soc_start_pct), Some(soc_end_pct)) = (session.soc_start, session.soc_end) else {
        return None;
    };
    if !session.complete || session.invalidated_reason.is_some() {
        return None;
    }
    Some(SessionRecord {
        pack_id: pack_sn.to_owned(),
        direction: match session.direction {
            LedgerDirection::Charge => BatteryDirection::Charge,
            LedgerDirection::Discharge => BatteryDirection::Discharge,
        },
        started_at: instant(session.started_at),
        ended_at: instant(session.ended_at),
        soc_start_pct,
        soc_end_pct,
        energy_ac_kwh: session.energy_ac_kwh,
        duration_s: session.duration_s as f64,
        sample_count: u32::try_from(session.sample_count).unwrap_or(u32::MAX),
        complete: session.complete,
    })
}

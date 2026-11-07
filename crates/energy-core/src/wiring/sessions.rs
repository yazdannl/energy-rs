//! Session re-extraction and capacity relearning after a completed run.

use energy_battery::control::Direction as BatteryDirection;
use energy_battery::learning::{
    CapacityBounds, LearningReport, LearningStore, SessionRecord, relearn,
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
    let packs = ledger.packs()?.all()?;
    let mut reports = Vec::new();
    for pack in &packs {
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
        let recent = policy.check_seconds().saturating_mul(2);
        let just_finished = sessions
            .iter()
            .any(|session| session.complete && session.ended_at >= now.saturating_sub(recent));
        if !just_finished {
            continue;
        }
        let records: Vec<SessionRecord> = sessions
            .iter()
            .filter_map(|session| record(session, &pack.sn))
            .collect();
        if records.is_empty() {
            continue;
        }
        let bounds = CapacityBounds::new(pack.label_capacity_kwh, pack.reported_capacity_kwh);
        let report = relearn(store, &pack.sn, &records, &bounds, instant(now))?;
        reports.push(report);
    }
    Ok(reports)
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

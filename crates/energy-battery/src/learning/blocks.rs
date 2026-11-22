//! Direction blocks: merge same-direction runs only when every boundary is continuous.
//!
//! A session that fails its own validity checks still exists as a direction change. It becomes an
//! unusable block, so a later charge can never be merged across it with an earlier charge.

use chrono::{DateTime, Utc};

use crate::control::Direction;
use crate::learning::efficiency::{
    CYCLE_TOLERANCE_PCT, HOLD_DRIFT_FRACTION, HOLD_DRIFT_PCT, MAX_PAIR_GAP_DAYS,
};
use crate::learning::session::SessionRecord;

/// A provably continuous run of same-direction sessions.
pub(crate) struct Block<'a> {
    pub(crate) direction: Direction,
    pub(crate) sessions: Vec<&'a SessionRecord>,
    pub(crate) energy_ac_kwh: f64,
    pub(crate) soc_start_pct: f64,
    pub(crate) soc_end_pct: f64,
    pub(crate) ended_at: DateTime<Utc>,
    /// False when any constituent session failed its own validity checks.
    pub(crate) usable: bool,
}

impl<'a> Block<'a> {
    fn new(session: &'a SessionRecord, usable: bool) -> Self {
        Self {
            direction: session.direction,
            sessions: vec![session],
            energy_ac_kwh: session.energy_ac_kwh,
            soc_start_pct: session.soc_start_pct,
            soc_end_pct: session.soc_end_pct,
            ended_at: session.ended_at,
            usable,
        }
    }

    fn extend(&mut self, session: &'a SessionRecord) {
        self.energy_ac_kwh += session.energy_ac_kwh;
        self.soc_end_pct = session.soc_end_pct;
        self.ended_at = session.ended_at;
        self.sessions.push(session);
    }

    pub(crate) fn span_pct(&self) -> f64 {
        match self.direction {
            Direction::Charge => self.soc_end_pct - self.soc_start_pct,
            Direction::Discharge => self.soc_start_pct - self.soc_end_pct,
        }
    }

    pub(crate) fn started_at(&self) -> DateTime<Utc> {
        self.sessions[0].started_at
    }

    pub(crate) fn refusal_slug(&self) -> Option<&'static str> {
        self.sessions
            .iter()
            .find_map(|session| session.refusal())
            .map(|reason| reason.slug())
    }
}

/// Every constituent boundary must pass the gap, hold-drift and SoC-progress guards.
fn continuity(a: &SessionRecord, b: &SessionRecord) -> Option<&'static str> {
    let gap_s = (b.started_at - a.ended_at).num_seconds();
    if gap_s < 0 {
        return Some("segment_overlap");
    }
    if gap_s as f64 > MAX_PAIR_GAP_DAYS * 86_400.0 {
        return Some("segment_gap_too_long");
    }
    let span = a.soc_span_pct().max(b.soc_span_pct());
    let drift = (b.soc_start_pct - a.soc_end_pct).abs();
    if drift > HOLD_DRIFT_PCT.max(HOLD_DRIFT_FRACTION * span) {
        return Some("segment_hold_drift");
    }
    let progressed = match a.direction {
        Direction::Charge => b.soc_end_pct - a.soc_end_pct,
        Direction::Discharge => a.soc_end_pct - b.soc_end_pct,
    };
    (progressed < -CYCLE_TOLERANCE_PCT).then_some("segment_soc_regression")
}

/// Group ordered sessions, reporting any same-direction boundary that had to split a block.
pub(crate) fn build_blocks<'a>(
    ordered: &[&'a SessionRecord],
    splits: &mut Vec<(&'static str, DateTime<Utc>)>,
) -> Vec<Block<'a>> {
    let mut grouped: Vec<Block<'a>> = Vec::new();
    for &session in ordered {
        let valid = session.refusal().is_none();
        let same = grouped
            .last()
            .is_some_and(|last| last.direction == session.direction && last.usable && valid);
        let boundary = if same {
            let prior = grouped
                .last()
                .and_then(|last| last.sessions.last().copied());
            prior.and_then(|previous| continuity(previous, session))
        } else {
            None
        };
        if same && boundary.is_none() {
            if let Some(last) = grouped.last_mut() {
                last.extend(session);
            }
            continue;
        }
        if let Some(reason) = boundary {
            splits.push((reason, session.started_at));
        }
        grouped.push(Block::new(session, valid));
    }
    grouped
}

//! Journal and ledger writes: plans, dispatch rows, live-write latch and events.

use energy_ledger::plan::{Plan as StoredPlan, PlanSlot};
use energy_ledger::{DispatchEntry, Event, EventLevel};

use crate::dispatch::PackReport;
use crate::engine::LATCH_TOLERANCE_W;
use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::engine::live_latch::LiveLatch;
use crate::plan::Plan;
use crate::safety::Authority;

impl Engine {
    /// Append one journal event.
    pub(crate) fn event(
        &self,
        at: i64,
        level: EventLevel,
        kind: &str,
        message: impl Into<String>,
    ) -> Result<(), EngineError> {
        let event = Event::new(at, level, kind, message);
        self.ledger.events()?.insert(&event)?;
        Ok(())
    }

    /// Store a plan header plus its 120 slots.
    pub(crate) fn store_plan(&self, plan: &Plan) -> Result<i64, EngineError> {
        let mode = self.gate.mode();
        let header = StoredPlan::new(
            plan.generated_at,
            mode.slug(),
            plan.horizon_start,
            plan.horizon_end,
            plan.summary.to_json(),
        );
        let slots: Vec<PlanSlot> = plan.slots.clone();
        let stored = self.ledger.plans()?.insert(&header, &slots)?;
        self.event(
            plan.generated_at,
            EventLevel::Info,
            "plan.generated",
            format!(
                "{} slots from {} (feasible {}, saved {:.3} EUR, forecast {})",
                plan.slots.len(),
                plan.horizon_start,
                plan.feasible,
                plan.summary.saved_eur,
                plan.degraded.as_deref().unwrap_or("fresh")
            ),
        )?;
        Ok(stored.id)
    }

    /// Persist one tick's dispatch rows and fold it into the live-write latch.
    pub(crate) fn record(
        &self,
        outcome: &crate::dispatch::TickOutcome,
        now: i64,
    ) -> Result<(), EngineError> {
        let mut state = self.lock()?;
        for report in &outcome.packs {
            self.insert_dispatch(report, outcome.mode, now)?;
            if note_latch(&mut state.latch, report, now) {
                state.latch_dirty = true;
            }
            if let Some(error) = &report.error {
                self.event(
                    now,
                    EventLevel::Warn,
                    "dispatch.fault",
                    format!("{}: {error}", report.pack_id),
                )?;
            }
        }
        if state.latch_dirty {
            state.latch.save(&self.ledger, now)?;
            state.latch_dirty = false;
        }
        note_selection(&self.ledger, &mut state.selection, outcome, now)?;
        Ok(())
    }

    /// One dispatch-log row.
    fn insert_dispatch(
        &self,
        report: &PackReport,
        mode: Authority,
        now: i64,
    ) -> Result<(), EngineError> {
        let mut entry = DispatchEntry::new(
            now,
            report.ledger_id,
            report.requested_w,
            report.applied_w,
            report.reason.as_str(),
            mode.slug(),
        );
        if let Some(note) = &report.note {
            entry = entry.with_note(note.clone());
        }
        if report.verified {
            entry = entry.verified(report.applied_w);
        } else if let Some(error) = &report.error {
            entry = entry.failed(error.clone());
        } else if !report.written {
            entry = entry.failed(format!(
                "not sent; intended {:.0} W ({})",
                report.intended_w,
                report.intended_reason.as_str()
            ));
        }
        self.ledger.dispatch()?.insert(&entry)?;
        Ok(())
    }

    /// Log a stale/expired plan, at most once every five minutes.
    pub(crate) fn log_stale(
        &self,
        now: i64,
        reason: crate::replan::ReplanReason,
    ) -> Result<(), EngineError> {
        let mut state = self.lock()?;
        if throttle(state.last_stale_log, now, 300) {
            state.last_stale_log = Some(now);
            drop(state);
            self.event(
                now,
                EventLevel::Warn,
                "dispatch.stale",
                format!("plan not executed ({}): holding all packs", reason.as_str()),
            )?;
        }
        Ok(())
    }

    /// Log a refused solve, at most once every five minutes.
    pub(crate) fn log_refusal(&self, now: i64, reason: &str) {
        if self.record_refusal(now, reason) {
            self.report_refusal(now, reason);
        }
    }

    /// Say it once: the operator's line in the add-on log, then the ledger event.
    fn report_refusal(&self, now: i64, reason: &str) {
        tracing::warn!(reason, "plan solve refused; holding all packs");
        if let Err(error) = self.event(now, EventLevel::Warn, "plan.refused", reason.to_owned()) {
            note_journal_failure(&error);
        }
    }

    /// Keep the refusal for the snapshot and the throttle; whether it is worth reporting now.
    fn record_refusal(&self, now: i64, reason: &str) -> bool {
        let Ok(mut state) = self.lock() else {
            return false;
        };
        if !throttle(state.last_refusal_log, now, 300) {
            return false;
        }
        state.last_refusal_log = Some(now);
        state.refusal = Some(crate::engine::Refusal {
            reason: reason.to_owned(),
            at: now,
        });
        true
    }
}

/// A ledger failure must not hide the refusal that is already in the log.
fn note_journal_failure(error: &EngineError) {
    tracing::warn!(%error, "plan refusal could not be journalled");
}

/// Fold one pack's outcome into the latch; returns whether it changed.
///
/// A dry-run or blocked row never latches. A verified neutral command clears; everything else that
/// reached a device with a non-zero setpoint latches, verified or not, because the device may be
/// executing it.
fn note_latch(latch: &mut LiveLatch, report: &PackReport, now: i64) -> bool {
    if !report.written {
        return false;
    }
    let watts = if report.fail_safe {
        0.0
    } else if report.verified {
        report.applied_w
    } else {
        report.intended_w
    };
    if report.verified && watts.abs() <= LATCH_TOLERANCE_W {
        return latch.clear(&report.pack_id);
    }
    if watts.abs() > LATCH_TOLERANCE_W {
        return latch.record(&report.pack_id, watts, now);
    }
    false
}

/// Whether a throttled log may fire again.
fn throttle(last: Option<i64>, now: i64, every_s: i64) -> bool {
    last.is_none_or(|at| now.saturating_sub(at) >= every_s)
}

/// Latch the pack that took the cycle, or release the hold when the cycle ended.
///
/// The hold is what stops SoC drift from rotating the choice on every replan, so it is written once
/// per cycle and cleared as soon as the fleet is idle or the direction flips
/// (`docs/PACK_SELECTION.md` §5). A dry-run tick never latches: nothing reached a device.
fn note_selection(
    ledger: &energy_ledger::Ledger,
    stored: &mut Option<energy_ledger::selection::SelectionRow>,
    outcome: &crate::dispatch::TickOutcome,
    now: i64,
) -> Result<(), EngineError> {
    let Some((pack_id, watts)) = lead(outcome) else {
        return release(ledger, stored);
    };
    let direction = if watts > 0.0 { "charge" } else { "discharge" };
    if stored
        .as_ref()
        .is_some_and(|row| row.holder == pack_id && row.direction == direction)
    {
        return Ok(());
    }
    if !outcome.mode.may_write() {
        // In a writing-disabled mode nothing is executed, so there is nothing to remember.
        return release(ledger, stored);
    }
    let row = energy_ledger::selection::SelectionRow {
        direction: direction.to_owned(),
        holder: pack_id.to_owned(),
        selected_at: now,
        target_w: outcome.target_w,
    };
    ledger.selection()?.set(&row)?;
    *stored = Some(row);
    Ok(())
}

/// The pack carrying the tick: the largest magnitude, id order breaking a tie.
fn lead(outcome: &crate::dispatch::TickOutcome) -> Option<(&str, f64)> {
    outcome
        .packs
        .iter()
        .filter(|report| report.intended_w.abs() > 0.0)
        .max_by(|left, right| {
            left.intended_w
                .abs()
                .total_cmp(&right.intended_w.abs())
                .then_with(|| right.pack_id.cmp(&left.pack_id))
        })
        .map(|report| (report.pack_id.as_str(), report.intended_w))
}

/// Drop the hold: the cycle is over (idle fleet, or nothing was executed).
fn release(
    ledger: &energy_ledger::Ledger,
    stored: &mut Option<energy_ledger::selection::SelectionRow>,
) -> Result<(), EngineError> {
    if stored.take().is_some() {
        ledger.selection()?.clear()?;
    }
    Ok(())
}

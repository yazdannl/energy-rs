//! The weekly (and manual) full-charge maintenance window.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDateTime, TimeZone, Utc, Weekday};
use chrono_tz::Europe::Amsterdam;
use chrono_tz::Tz;

use crate::protection::full_charge::action::FullChargeAction;
use crate::protection::full_charge::settings::FullChargeSettings;

/// SoC within this many points of the target counts as reached: a pack rarely sits on exactly 100 %,
/// and the point is the balance window, not the last digit.
pub const TOLERANCE_PCT: f64 = 1.0;
/// A charge run that has not reached the target after this long is released, so a pack that cannot
/// follow the plan (an empty PV day, a clipped device) cannot wedge the scheduler forever.
pub const MAX_RUN_HOURS: f64 = 24.0;
/// The schedule is a local calendar event, so it is evaluated in the operator's zone.
pub const ZONE: Tz = Amsterdam;

/// The maintenance phase the scheduler is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Waiting for the next weekly run or a manual request.
    Idle,
    /// Charging towards the target.
    Charging,
    /// At the target, holding so the BMS can balance.
    Holding,
}

/// Weekly (plus manual) full-charge maintenance, DST-safe and clock-free.
///
/// The scheduler answers one question per tick: "what should the maintenance override be?" It never
/// reads a clock (the caller passes `now`), never talks to a device, and exposes `next_run`/
/// `last_run` for the UI. Everything is in local `Europe/Amsterdam` time: a weekly run is a calendar
/// event, so adding seven *local* days keeps its wall-clock time across both DST transitions, which
/// absolute-time arithmetic would not.
#[derive(Debug, Clone, PartialEq)]
pub struct FullChargeScheduler {
    /// The operator's settings; call [`Self::reschedule`] after changing the schedule.
    pub settings: FullChargeSettings,
    stage: Stage,
    next_run: Option<DateTime<Utc>>,
    last_run: Option<DateTime<Utc>>,
    hold_until: Option<DateTime<Utc>>,
    manual: bool,
}

impl FullChargeScheduler {
    /// A scheduler for the given settings, with the next weekly run computed from `now`.
    pub fn new(settings: FullChargeSettings, now: DateTime<Utc>) -> Self {
        let mut scheduler = Self {
            settings,
            stage: Stage::Idle,
            next_run: None,
            last_run: None,
            hold_until: None,
            manual: false,
        };
        scheduler.reschedule(now);
        scheduler
    }

    /// When the next weekly run is due, or `None` while maintenance is disabled.
    pub fn next_run(&self) -> Option<DateTime<Utc>> {
        self.next_run
    }

    /// When the current (or last) run started.
    pub fn last_run(&self) -> Option<DateTime<Utc>> {
        self.last_run
    }

    /// Whether a run is in progress (charging or holding).
    pub fn is_running(&self) -> bool {
        self.stage != Stage::Idle
    }

    /// Ask for a run at the next evaluation, whatever the schedule says.
    ///
    /// A manual run also moves `next_run` a week out, because a completed maintenance is a completed
    /// maintenance: the schedule starts counting from the run that actually happened.
    pub fn request_manual(&mut self) {
        self.manual = true;
    }

    /// Recompute the next weekly run from `now` (call after editing the settings).
    pub fn reschedule(&mut self, now: DateTime<Utc>) {
        self.next_run = self.settings.enabled.then(|| self.weekly_after(now));
    }

    /// One tick: what the maintenance override should be now.
    ///
    /// `soc_pct` is the measured SoC (`None` counts as "not at the target": the pack has to prove it
    /// arrived). `ceiling_w` is the caller's own charge ceiling, so the answer is already the minimum
    /// of the maintenance setting and the control clamp. Calling this twice at the same instant is
    /// idempotent: a phase transition happens once, and `last_run` only moves when a run starts.
    pub fn evaluate(
        &mut self,
        now: DateTime<Utc>,
        soc_pct: Option<f64>,
        ceiling_w: u32,
    ) -> FullChargeAction {
        if !self.settings.enabled {
            return FullChargeAction::Idle;
        }
        let limit_w = self.settings.max_charge_w.min(ceiling_w);
        if self.manual {
            self.manual = false;
            self.begin(now);
        } else if self.stage == Stage::Idle && self.next_run.is_some_and(|next| now >= next) {
            self.begin(now);
        }
        match self.stage {
            Stage::Idle => FullChargeAction::Idle,
            Stage::Charging => self.charging(now, soc_pct, limit_w),
            Stage::Holding => self.holding(now, soc_pct, limit_w),
        }
    }

    /// Start a run: stamp it, and push the weekly schedule a week past it.
    fn begin(&mut self, now: DateTime<Utc>) {
        self.stage = Stage::Charging;
        self.last_run = Some(now);
        self.hold_until = None;
        let next = self.weekly_after(now);
        self.next_run = Some(next);
    }

    /// Charging towards the target, then the hold window.
    fn charging(
        &mut self,
        now: DateTime<Utc>,
        soc_pct: Option<f64>,
        limit_w: u32,
    ) -> FullChargeAction {
        if self.expired(now) {
            return self.release();
        }
        let target = self.settings.target();
        if reached(soc_pct, target) {
            let hold = Duration::minutes((self.settings.hold() * 60.0).round() as i64);
            let until = now + hold;
            self.stage = Stage::Holding;
            self.hold_until = Some(until);
            return FullChargeAction::Hold { limit_w, until };
        }
        FullChargeAction::Charge {
            limit_w,
            target_soc_pct: target,
        }
    }

    /// Holding at the target until the window ends; a pack that slips off the target resumes.
    fn holding(
        &mut self,
        now: DateTime<Utc>,
        soc_pct: Option<f64>,
        limit_w: u32,
    ) -> FullChargeAction {
        let target = self.settings.target();
        if !reached(soc_pct, target) {
            self.stage = Stage::Charging;
            self.hold_until = None;
            return FullChargeAction::Charge {
                limit_w,
                target_soc_pct: target,
            };
        }
        let until = self.hold_until.unwrap_or(now);
        if now >= until {
            return self.release();
        }
        FullChargeAction::Hold { limit_w, until }
    }

    /// Leave the maintenance override behind, once.
    fn release(&mut self) -> FullChargeAction {
        self.stage = Stage::Idle;
        self.hold_until = None;
        FullChargeAction::Release
    }

    /// Whether a charging run has been waiting for the target for too long.
    fn expired(&self, now: DateTime<Utc>) -> bool {
        self.last_run
            .is_some_and(|start| (now - start).num_seconds() as f64 > MAX_RUN_HOURS * 3600.0)
    }

    /// The next occurrence of the configured local weekday and time, strictly after `after`.
    ///
    /// The local date is advanced by seven *local* days, so the wall-clock time is preserved across
    /// both DST transitions instead of drifting by an hour. A wall clock that does not exist (the
    /// spring-forward gap) resolves forward to the first hour that does; an ambiguous one (the
    /// autumn overlap) resolves to the earlier instant.
    fn weekly_after(&self, after: DateTime<Utc>) -> DateTime<Utc> {
        let local = after.with_timezone(&ZONE);
        let mut date = local.date_naive();
        date += Duration::days(i64::from(days_until(date.weekday(), self.settings.weekday)));
        for _ in 0..3 {
            let candidate = resolve(date.and_time(self.settings.at));
            if candidate > after {
                return candidate;
            }
            date += Duration::days(7);
        }
        after + Duration::days(7)
    }
}

/// Whole days from `from` to `to` in the same week (`0` when they are the same day).
fn days_until(from: Weekday, to: Weekday) -> u32 {
    (to.num_days_from_monday() + 7 - from.num_days_from_monday()) % 7
}

/// Whether the measured SoC has reached the target within the tolerance.
fn reached(soc_pct: Option<f64>, target_soc_pct: f64) -> bool {
    soc_pct.is_some_and(|soc| soc.is_finite() && soc >= target_soc_pct - TOLERANCE_PCT)
}

/// A local wall clock in this zone as a UTC instant.
///
/// A nonexistent local time (spring forward) is moved forward in one-hour steps until the wall clock
/// exists; an ambiguous one (autumn overlap) takes the earlier of the two instants. Both choices are
/// deterministic and in the safe direction: maintenance never runs *earlier* than the requested wall
/// clock, and never twice.
fn resolve(naive: NaiveDateTime) -> DateTime<Utc> {
    match ZONE.from_local_datetime(&naive) {
        LocalResult::Single(moment) => moment.with_timezone(&Utc),
        LocalResult::Ambiguous(earlier, _later) => earlier.with_timezone(&Utc),
        LocalResult::None => {
            let mut shifted = naive;
            for _ in 0..3 {
                shifted += Duration::hours(1);
                if let Some(moment) = ZONE.from_local_datetime(&shifted).earliest() {
                    return moment.with_timezone(&Utc);
                }
            }
            naive.and_utc()
        }
    }
}

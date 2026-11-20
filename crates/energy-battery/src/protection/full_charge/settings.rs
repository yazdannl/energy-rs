//! When a maintenance full charge runs and what it asks for.

use chrono::{NaiveTime, Weekday};

/// The operator's full-charge maintenance settings.
///
/// Off by default: a maintenance charge is a real charge cycle, so it is opted into. The wall-clock
/// fields are local (`Europe/Amsterdam`) because the run is a calendar event, not a UTC instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FullChargeSettings {
    /// Whether scheduled and manual maintenance may run at all.
    pub enabled: bool,
    /// Local weekday the weekly run starts on.
    pub weekday: Weekday,
    /// Local wall-clock time the run starts at.
    pub at: NaiveTime,
    /// SoC the pack must reach before the hold window starts.
    pub target_soc_pct: f64,
    /// How long the pack is held at the target so the BMS can balance, hours.
    pub hold_hours: f64,
    /// Ceiling for the requested charge power, W; the control clamp still applies downstream.
    pub max_charge_w: u32,
    /// Days between completed balancing dwells, per pack. Clamped to 1..=60.
    pub interval_days: u32,
    /// Minutes a pack is held at 100 % so the BMS can balance. Clamped to 15..=240.
    pub dwell_minutes: u32,
}

/// Shipped default days between maintenance charges.
pub const DEFAULT_INTERVAL_DAYS: u32 = 7;
/// Shipped default balancing dwell, minutes.
pub const DEFAULT_DWELL_MINUTES: u32 = 60;
/// Earliest a replan may schedule a maintenance charge again.
pub const MIN_INTERVAL_DAYS: u32 = 1;
/// Latest a replan may defer a maintenance charge.
pub const MAX_INTERVAL_DAYS: u32 = 60;
/// Shortest useful balancing dwell.
pub const MIN_DWELL_MINUTES: u32 = 15;
/// A longer dwell than this is a stuck override, not balancing.
pub const MAX_DWELL_MINUTES: u32 = 240;

impl FullChargeSettings {
    /// The shipped defaults: Sunday 03:00 local, 100 %, a six-hour hold, 1000 W.
    pub fn weekly(max_charge_w: u32) -> Self {
        Self {
            enabled: true,
            weekday: Weekday::Sun,
            at: NaiveTime::from_hms_opt(3, 0, 0).unwrap_or_default(),
            target_soc_pct: 100.0,
            hold_hours: 6.0,
            max_charge_w,
            interval_days: DEFAULT_INTERVAL_DAYS,
            dwell_minutes: DEFAULT_DWELL_MINUTES,
        }
    }

    /// The interval in days, clamped to the supported range.
    pub fn interval_days(&self) -> u32 {
        self.interval_days
            .clamp(MIN_INTERVAL_DAYS, MAX_INTERVAL_DAYS)
    }

    /// The balancing dwell in minutes, clamped to the supported range.
    pub fn dwell_minutes(&self) -> u32 {
        self.dwell_minutes
            .clamp(MIN_DWELL_MINUTES, MAX_DWELL_MINUTES)
    }

    /// The hold window in hours, clamped to a sane range so the `chrono::Duration` arithmetic
    /// cannot overflow on a nonsense setting.
    pub fn hold(&self) -> f64 {
        if self.hold_hours.is_finite() {
            self.hold_hours.clamp(0.0, 48.0)
        } else {
            0.0
        }
    }

    /// The target clamped to the range a pack may be pushed to.
    pub fn target(&self) -> f64 {
        if self.target_soc_pct.is_finite() {
            self.target_soc_pct.clamp(50.0, 100.0)
        } else {
            100.0
        }
    }
}

// The zero value of every field is not a usable maintenance plan (target 0 %, 0 W).
#[allow(clippy::derivable_impls)]
impl Default for FullChargeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            ..Self::weekly(1000)
        }
    }
}

//! When the weekly full charge runs and what it asks for.

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
}

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
        }
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

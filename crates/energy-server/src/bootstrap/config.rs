//! The engine policy, from the stored settings with the shipped defaults.
//!
//! Settings are operator-owned and stored as text; this is the one place that turns them into the
//! typed [`EngineConfig`]. An unreadable or missing value falls back to the default instead of
//! refusing to boot: a corrupt reserve percentage must not keep the battery uncontrolled, and the
//! default is the safe, documented one.

use chrono::{NaiveTime, Weekday};
use energy_battery::protection::CellConfig;
use energy_battery::protection::full_charge::FullChargeSettings;
use energy_core::engine::EngineConfig;
use energy_ledger::Ledger;

use crate::bootstrap::options::Options;

/// Build the engine policy from the ledger's settings and the add-on mode.
pub fn engine_config(options: &Options, ledger: &Ledger) -> EngineConfig {
    let defaults = EngineConfig::default();
    let max_charge_w = number(ledger, "battery_power_limit_w", 0.0);
    EngineConfig {
        mode: options.mode(),
        tick_seconds: options.tick_seconds(),
        replan_seconds: seconds(ledger, "replan_seconds", defaults.replan_seconds),
        stale_plan_seconds: seconds(ledger, "stale_input_seconds", defaults.stale_plan_seconds),
        reserve_soc_pct: number(ledger, "reserve_soc_pct", defaults.reserve_soc_pct),
        max_grid_import_w: number(ledger, "grid_import_limit_w", defaults.max_grid_import_w),
        max_grid_export_w: number(ledger, "grid_export_limit_w", defaults.max_grid_export_w),
        fleet_max_charge_w: max_charge_w,
        fleet_max_discharge_w: max_charge_w,
        cell: CellConfig::default(),
        full_charge: full_charge(ledger, max_charge_w),
        ..defaults
    }
}

/// The weekly maintenance window from `full_charge_weekday`/`full_charge_time`.
#[allow(clippy::cast_sign_loss)] // clamped to a positive ceiling above the cast
fn full_charge(ledger: &Ledger, limit_w: f64) -> FullChargeSettings {
    let ceiling = limit_w.clamp(800.0, 4_000.0);
    let mut settings = FullChargeSettings::weekly(ceiling.round() as u32);
    if let Some(day) = text(ledger, "full_charge_weekday").and_then(|value| weekday(&value)) {
        settings.weekday = day;
    }
    if let Some(at) = text(ledger, "full_charge_time").and_then(|value| time(&value)) {
        settings.at = at;
    }
    settings
}

fn weekday(value: &str) -> Option<Weekday> {
    match value.trim() {
        "mon" => Some(Weekday::Mon),
        "tue" => Some(Weekday::Tue),
        "wed" => Some(Weekday::Wed),
        "thu" => Some(Weekday::Thu),
        "fri" => Some(Weekday::Fri),
        "sat" => Some(Weekday::Sat),
        "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

fn time(value: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(value.trim(), "%H:%M").ok()
}

fn text(ledger: &Ledger, key: &str) -> Option<String> {
    ledger
        .settings()
        .and_then(|settings| settings.get(key))
        .ok()
        .flatten()
}

fn number(ledger: &Ledger, key: &str, fallback: f64) -> f64 {
    text(ledger, key)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

fn seconds(ledger: &Ledger, key: &str, fallback: i64) -> i64 {
    text(ledger, key)
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

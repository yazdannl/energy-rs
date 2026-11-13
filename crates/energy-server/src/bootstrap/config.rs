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
use energy_core::plan::{ExportPolicy, Policy};
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
        solar_reserve_pct: number(ledger, "solar_reserve_pct", 20.0),
        min_pack_power_w: number(ledger, "min_pack_power_w", 50.0),
        pack_staggering: text(ledger, "pack_staggering").as_deref() != Some("off"),
        pack_switch_soc_pct: number(ledger, "pack_switch_soc_pct", 2.0),
        pack_hold_seconds: seconds(ledger, "pack_hold_seconds", defaults.pack_hold_seconds),
        grid_follow_deadband_w: number(ledger, "grid_follow_deadband_w", 10.0),
        max_grid_import_w: number(ledger, "grid_import_limit_w", defaults.max_grid_import_w),
        max_grid_export_w: number(ledger, "grid_export_limit_w", defaults.max_grid_export_w),
        fleet_max_charge_w: max_charge_w,
        fleet_max_discharge_w: max_charge_w,
        cell: CellConfig::default(),
        full_charge: full_charge(ledger, max_charge_w),
        ..defaults
    }
}

/// The post-net-metering policy from the stored settings.
pub fn policy(ledger: &Ledger) -> Policy {
    let basis = number(ledger, "cost_basis_eur_per_kwh", 0.0);
    Policy {
        export: ExportPolicy::parse(&text(ledger, "export_policy").unwrap_or_default()),
        solar_reserve_pct: number(ledger, "solar_reserve_pct", 20.0),
        peak_reserve_pct: number(ledger, "peak_reserve_pct", 20.0),
        peak_reserve_value_eur_per_kwh: number(ledger, "peak_reserve_value_eur_per_kwh", 0.15),
        wear_eur_per_kwh: number(ledger, "wear_eur_per_kwh", 0.02),
        cost_basis_eur_per_kwh: (basis > 0.0).then_some(basis),
        zero_export: text(ledger, "zero_export").as_deref() == Some("on"),
        net_metering: net_metering(text(ledger, "net_metering").as_deref()),
    }
}

/// The PV array and forecast mode from the stored settings.
///
/// An unconfigured array (`dc_kwp` 0) means the learned model cannot run, so `auto` falls back to
/// the provider. The defaults describe this install's plane, not a generic one: a wrong tilt is a
/// wrong forecast, and a zero is honest.
pub fn pv(ledger: &Ledger) -> energy_core::plan_source::PvConfig {
    energy_core::plan_source::PvConfig {
        dc_kwp: number(ledger, "pv_dc_kwp", 0.0),
        ac_kwp: number(ledger, "pv_ac_kwp", 0.0),
        tilt_deg: number(ledger, "pv_tilt_deg", 25.0),
        azimuth_deg: number(ledger, "pv_azimuth_deg", 135.0),
        mode: energy_core::plan_source::PvMode::parse(&text(ledger, "pv_mode").unwrap_or_default()),
        scale: number(ledger, "pv_scale", 1.0),
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

/// The net-metering regime: the operator's switch, on unless it was turned off.
///
/// Not date-driven on purpose: whether a contract still nets 1:1 is the contract's business, and a
/// calendar that flips the price model under the plan is a trap. The default is on, which is the
/// 2026 reality; turn it off when the contract does.
fn net_metering(value: Option<&str>) -> energy_price::settlement::NetMetering {
    use energy_price::settlement::NetMetering;
    match value {
        Some("off") => NetMetering::Off,
        _ => NetMetering::On,
    }
}

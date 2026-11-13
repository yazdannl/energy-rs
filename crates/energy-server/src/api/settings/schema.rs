//! The writable-settings whitelist.
//!
//! Anything not listed here is invisible to the API on purpose: tokens, battery credentials and
//! other private keys live in add-on options, never in the HTTP contract. Adding a setting means
//! adding one line here, its validation kind, and a default.

use super::setting_kind::SettingKind;
use super::setting_spec::SettingSpec;

const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const PRICE_PROVIDERS: [&str; 2] = ["zonneplan", "ha_sensor"];
const EXPORT_POLICIES: [&str; 2] = ["legacy", "cost_basis"];
const ON_OFF: [&str; 2] = ["off", "on"];
const PV_MODES: [&str; 3] = ["auto", "model", "provider"];
const NET_METERING: [&str; 2] = ["on", "off"];

/// Every writable setting, in UI order.
static SPECS: [SettingSpec; 30] = [
    SettingSpec {
        key: "grid_import_limit_w",
        kind: SettingKind::Number {
            min: 0.0,
            max: 20_000.0,
        },
        default: "0",
    },
    SettingSpec {
        key: "grid_export_limit_w",
        kind: SettingKind::Number {
            min: 0.0,
            max: 20_000.0,
        },
        default: "0",
    },
    SettingSpec {
        key: "reserve_soc_pct",
        kind: SettingKind::Number {
            min: 5.0,
            max: 50.0,
        },
        default: "10",
    },
    SettingSpec {
        key: "max_soc_pct",
        kind: SettingKind::Number {
            min: 80.0,
            max: 100.0,
        },
        default: "100",
    },
    SettingSpec {
        key: "battery_power_limit_w",
        kind: SettingKind::Number {
            min: 0.0,
            max: 4_000.0,
        },
        default: "0",
    },
    SettingSpec {
        key: "price_provider",
        kind: SettingKind::Choice(&PRICE_PROVIDERS),
        default: "zonneplan",
    },
    SettingSpec {
        key: "price_entity",
        kind: SettingKind::Text,
        default: "",
    },
    SettingSpec {
        key: "replan_seconds",
        kind: SettingKind::Number {
            min: 60.0,
            max: 3_600.0,
        },
        default: "300",
    },
    SettingSpec {
        key: "stale_input_seconds",
        kind: SettingKind::Number {
            min: 60.0,
            max: 7_200.0,
        },
        default: "900",
    },
    SettingSpec {
        key: "full_charge_weekday",
        kind: SettingKind::Choice(&WEEKDAYS),
        default: "sun",
    },
    SettingSpec {
        key: "full_charge_time",
        kind: SettingKind::Text,
        default: "03:00",
    },
    SettingSpec {
        key: "solar_reserve_pct",
        kind: SettingKind::Number {
            min: 0.0,
            max: 50.0,
        },
        default: "20",
    },
    SettingSpec {
        key: "peak_reserve_pct",
        kind: SettingKind::Number {
            min: 0.0,
            max: 50.0,
        },
        default: "20",
    },
    SettingSpec {
        key: "peak_reserve_value_eur_per_kwh",
        kind: SettingKind::Number { min: 0.0, max: 1.0 },
        default: "0.15",
    },
    SettingSpec {
        key: "wear_eur_per_kwh",
        kind: SettingKind::Number { min: 0.0, max: 0.1 },
        default: "0.02",
    },
    SettingSpec {
        key: "cost_basis_eur_per_kwh",
        kind: SettingKind::Number { min: 0.0, max: 1.0 },
        default: "0",
    },
    SettingSpec {
        key: "min_pack_power_w",
        kind: SettingKind::Number {
            min: 0.0,
            max: 500.0,
        },
        default: "50",
    },
    SettingSpec {
        key: "pack_staggering",
        kind: SettingKind::Choice(&ON_OFF),
        default: "on",
    },
    SettingSpec {
        key: "pack_switch_soc_pct",
        kind: SettingKind::Number {
            min: 0.0,
            max: 20.0,
        },
        default: "2",
    },
    SettingSpec {
        key: "pack_hold_seconds",
        kind: SettingKind::Number {
            min: 60.0,
            max: 14_400.0,
        },
        default: "1800",
    },
    SettingSpec {
        key: "grid_follow_deadband_w",
        kind: SettingKind::Number {
            min: 0.0,
            max: 500.0,
        },
        default: "10",
    },
    SettingSpec {
        key: "net_metering",
        kind: SettingKind::Choice(&NET_METERING),
        default: "on",
    },
    SettingSpec {
        key: "zero_export",
        kind: SettingKind::Choice(&ON_OFF),
        default: "off",
    },
    SettingSpec {
        key: "export_policy",
        kind: SettingKind::Choice(&EXPORT_POLICIES),
        default: "legacy",
    },
    SettingSpec {
        key: "pv_mode",
        kind: SettingKind::Choice(&PV_MODES),
        default: "auto",
    },
    SettingSpec {
        key: "pv_dc_kwp",
        kind: SettingKind::Number {
            min: 0.0,
            max: 100.0,
        },
        default: "0",
    },
    SettingSpec {
        key: "pv_ac_kwp",
        kind: SettingKind::Number {
            min: 0.0,
            max: 100.0,
        },
        default: "0",
    },
    SettingSpec {
        key: "pv_tilt_deg",
        kind: SettingKind::Number {
            min: 0.0,
            max: 90.0,
        },
        default: "25",
    },
    SettingSpec {
        key: "pv_azimuth_deg",
        kind: SettingKind::Number {
            min: 0.0,
            max: 359.999,
        },
        default: "135",
    },
    SettingSpec {
        key: "pv_scale",
        kind: SettingKind::Number { min: 0.1, max: 5.0 },
        default: "1",
    },
];

/// Every spec, in UI order.
pub(crate) fn specs() -> &'static [SettingSpec] {
    &SPECS
}

/// Look one spec up by key.
pub(crate) fn spec(key: &str) -> Option<&'static SettingSpec> {
    SPECS.iter().find(|spec| spec.key == key)
}

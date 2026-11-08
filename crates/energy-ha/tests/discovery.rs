// Test code: a failed assertion is the report, so `unwrap`/`expect`/`panic!` are the intended
// API here.  The workspace policy (no panics in shipped code) applies to the library.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::float_cmp,
    clippy::manual_midpoint
)]

mod support;

use std::collections::HashMap;

use energy_ha::{
    Config, Connection, EnergyPreferences, PowerReading, SiteModel, discover, fetch_prefs, to_watts,
};
use serde_json::{Value, json};
use support::{FakeServer, TOKEN};

fn prefs(value: Value) -> EnergyPreferences {
    serde_json::from_value(value).expect("prefs")
}

fn reading(value: f64, unit: &str) -> PowerReading {
    PowerReading {
        value,
        unit: unit.to_owned(),
    }
}

fn readings(items: &[(&str, f64, &str)]) -> HashMap<String, PowerReading> {
    items
        .iter()
        .map(|(entity, value, unit)| ((*entity).to_owned(), reading(*value, unit)))
        .collect()
}

#[test]
fn fixture_mapping_uses_stat_rate_when_present() {
    let prefs: EnergyPreferences =
        serde_json::from_value(support::result("energy_prefs.json")).expect("prefs");
    let site = SiteModel::from_prefs(&prefs);

    assert!(site.configured);
    assert!(site.grid.import.is_some());
    assert!(site.grid.export.is_some());
    assert_ne!(site.grid.import, site.grid.export);
    assert!(matches!(
        site.grid.power,
        Some(energy_ha::PowerRule::Preferred { .. })
    ));
    assert!(site.solar.energy.is_some());
    assert!(matches!(
        site.solar.power,
        Some(energy_ha::PowerRule::Direct(_))
    ));
    assert_eq!(site.solar.forecast_entry_ids.len(), 2);
}

#[test]
fn fixture_mapping_maps_batteries_and_devices() {
    let prefs: EnergyPreferences =
        serde_json::from_value(support::result("energy_prefs.json")).expect("prefs");
    let site = SiteModel::from_prefs(&prefs);

    assert_eq!(site.batteries.len(), 4);
    for battery in &site.batteries {
        assert!(battery.discharge.is_some());
        assert!(battery.charge.is_some());
        assert!(battery.soc.is_some());
        assert_eq!(battery.capacity, Some(2.06));
        assert!(
            battery.power.is_none(),
            "this dashboard has no battery live power"
        );
    }

    assert_eq!(site.devices.len(), 1);
    let device = &site.devices[0];
    assert!(device.name.is_some());
    assert!(device.consumption.is_some());
    assert!(matches!(
        device.power,
        Some(energy_ha::PowerRule::Direct(_))
    ));
    assert!(device.included_in_stat.is_none());
}

#[test]
fn grid_difference_uses_import_minus_export() {
    let site = SiteModel::from_prefs(&prefs(json!({
        "energy_sources": [{
            "type": "grid",
            "stat_energy_from": "sensor.grid_import",
            "stat_energy_to": "sensor.grid_export",
            "power_config": {
                "stat_rate_from": "sensor.grid_from",
                "stat_rate_to": "sensor.grid_to"
            }
        }, { "type": "gas", "stat_energy_from": "sensor.gas" }]
    })));
    assert!(site.configured);
    assert_eq!(
        site.grid.power,
        Some(energy_ha::PowerRule::Difference {
            from: "sensor.grid_from".to_owned(),
            to: "sensor.grid_to".to_owned(),
        })
    );

    let import = readings(&[
        ("sensor.grid_from", 1_000.0, "W"),
        ("sensor.grid_to", 250.0, "W"),
    ]);
    assert_eq!(
        site.grid
            .power
            .as_ref()
            .and_then(|rule| rule.resolve(&import)),
        Some(750.0)
    );

    let export = readings(&[
        ("sensor.grid_from", 50.0, "kW"),
        ("sensor.grid_to", 200.0, "W"),
    ]);
    assert_eq!(
        site.grid
            .power
            .as_ref()
            .and_then(|rule| rule.resolve(&export)),
        Some(49_800.0)
    );
}

#[test]
fn battery_difference_uses_discharge_minus_charge() {
    let site = SiteModel::from_prefs(&prefs(json!({
        "energy_sources": [{
            "type": "battery",
            "stat_energy_from": "sensor.battery_discharge",
            "stat_energy_to": "sensor.battery_charge",
            "stat_soc": "sensor.battery_soc",
            "power_config": {
                "stat_rate_from": "sensor.battery_from",
                "stat_rate_to": "sensor.battery_to"
            }
        }]
    })));
    assert_eq!(
        site.batteries[0].power,
        Some(energy_ha::PowerRule::Difference {
            from: "sensor.battery_from".to_owned(),
            to: "sensor.battery_to".to_owned(),
        })
    );

    let battery = readings(&[
        ("sensor.battery_from", 400.0, "W"),
        ("sensor.battery_to", 100.0, "W"),
    ]);
    assert_eq!(
        site.batteries[0]
            .power
            .as_ref()
            .and_then(|rule| rule.resolve(&battery)),
        Some(300.0)
    );
}

#[test]
fn preferred_power_rule_falls_back_to_power_config() {
    let site = SiteModel::from_prefs(&prefs(json!({
        "energy_sources": [{
            "type": "grid",
            "stat_energy_from": "sensor.grid_import",
            "stat_energy_to": "sensor.grid_export",
            "stat_rate": "sensor.grid_net",
            "power_config": {
                "stat_rate_from": "sensor.grid_from",
                "stat_rate_to": "sensor.grid_to"
            }
        }]
    })));
    let rule = site.grid.power.as_ref().expect("rule");
    assert_eq!(
        rule.resolve(&readings(&[
            ("sensor.grid_net", 900.0, "W"),
            ("sensor.grid_from", 500.0, "W"),
            ("sensor.grid_to", 100.0, "W"),
        ])),
        Some(900.0),
        "stat_rate wins when it resolves"
    );
    assert_eq!(
        rule.resolve(&readings(&[
            ("sensor.grid_from", 500.0, "W"),
            ("sensor.grid_to", 100.0, "W"),
        ])),
        Some(400.0),
        "power_config is recomputed when stat_rate has no reading"
    );
}

#[test]
fn inverted_power_and_units_are_resolved_in_watts() {
    let site = SiteModel::from_prefs(&prefs(json!({
        "energy_sources": [{
            "type": "battery",
            "stat_energy_from": "sensor.discharge",
            "stat_energy_to": "sensor.charge",
            "power_config": { "stat_rate_inverted": "sensor.flow" }
        }]
    })));
    assert_eq!(
        site.batteries[0].power,
        Some(energy_ha::PowerRule::Inverted("sensor.flow".to_owned()))
    );
    let values = readings(&[("sensor.flow", 300.0, "W")]);
    assert_eq!(
        site.batteries[0]
            .power
            .as_ref()
            .and_then(|rule| rule.resolve(&values)),
        Some(-300.0)
    );

    assert_eq!(to_watts(1.2, "kW"), Some(1_200.0));
    assert_eq!(to_watts(0.5, "MW"), Some(500_000.0));
    assert_eq!(to_watts(250.0, "mW"), Some(0.25));
    assert_eq!(to_watts(1.0, "furlongs"), None);
}

#[test]
fn multiple_arrays_sum_strictly_and_missing_is_not_zero() {
    let site = SiteModel::from_prefs(&prefs(json!({
        "energy_sources": [
            { "type": "solar", "stat_energy_from": "sensor.pv_a", "stat_rate": "sensor.pv_a_w" },
            { "type": "solar", "stat_energy_from": "sensor.pv_b", "stat_rate": "sensor.pv_b_w" },
            {
                "type": "battery",
                "stat_energy_from": "sensor.one_out",
                "stat_energy_to": "sensor.one_in",
                "stat_rate": "sensor.one_w"
            },
            {
                "type": "battery",
                "stat_energy_from": "sensor.two_out",
                "stat_energy_to": "sensor.two_in"
            }
        ]
    })));
    assert!(matches!(
        site.solar.power,
        Some(energy_ha::PowerRule::Sum(_))
    ));

    let both = readings(&[("sensor.pv_a_w", 1.0, "kW"), ("sensor.pv_b_w", 500.0, "W")]);
    assert_eq!(
        site.solar
            .power
            .as_ref()
            .and_then(|rule| rule.resolve(&both)),
        Some(1_500.0)
    );
    let partial = readings(&[("sensor.pv_a_w", 1.0, "kW")]);
    assert_eq!(
        site.solar
            .power
            .as_ref()
            .and_then(|rule| rule.resolve(&partial)),
        None,
        "a missing array must not silently under-count"
    );

    let one_battery = readings(&[("sensor.one_w", 400.0, "W")]);
    let live = site.live(&one_battery);
    assert_eq!(live.battery_w, None, "battery total is strict");
}

#[test]
fn soc_is_capacity_weighted_and_excludes_invalid_values() {
    let site = SiteModel::from_prefs(&prefs(json!({
        "energy_sources": [
            {
                "type": "battery",
                "stat_energy_from": "sensor.a_out",
                "stat_energy_to": "sensor.a_in",
                "stat_soc": "sensor.a_soc",
                "capacity": 5.0
            },
            {
                "type": "battery",
                "stat_energy_from": "sensor.b_out",
                "stat_energy_to": "sensor.b_in",
                "stat_soc": "sensor.b_soc",
                "capacity": 2.5
            },
            {
                "type": "battery",
                "stat_energy_from": "sensor.c_out",
                "stat_energy_to": "sensor.c_in",
                "stat_soc": "sensor.c_soc"
            }
        ]
    })));
    let values = readings(&[
        ("sensor.a_soc", 50.0, "%"),
        ("sensor.b_soc", 100.0, "%"),
        ("sensor.c_soc", 40.0, "%"),
    ]);
    let expected = (50.0 * 5.0 + 100.0 * 2.5 + 40.0 * 3.75) / (5.0 + 2.5 + 3.75);
    assert!((site.soc_pct(&values).expect("soc") - expected).abs() < 1e-9);

    let invalid = readings(&[("sensor.a_soc", 150.0, "%")]);
    assert_eq!(site.soc_pct(&invalid), None);
}

#[test]
fn live_balance_subtracts_a_charging_battery_and_never_zero_fills() {
    let site = SiteModel::from_prefs(&prefs(json!({
        "energy_sources": [
            { "type": "grid", "stat_energy_from": "sensor.grid", "stat_rate": "sensor.grid_w" },
            { "type": "solar", "stat_energy_from": "sensor.pv", "stat_rate": "sensor.pv_w" },
            {
                "type": "battery",
                "stat_energy_from": "sensor.out",
                "stat_energy_to": "sensor.in",
                "stat_rate": "sensor.battery_w",
                "stat_soc": "sensor.soc",
                "capacity": 2.0
            }
        ]
    })));
    // A negative battery power discharges into the house, so it adds to the load.
    let discharging = readings(&[
        ("sensor.grid_w", 100.0, "W"),
        ("sensor.pv_w", 300.0, "W"),
        ("sensor.battery_w", -500.0, "W"),
        ("sensor.soc", 55.0, "%"),
    ]);
    let live = site.live(&discharging);
    assert_eq!(live.load_w, Some(900.0));
    assert!(!live.load_clamped);
    assert_eq!(live.soc_pct, Some(55.0));

    // A positive battery power charges, so it comes out of the load.
    let charging = readings(&[
        ("sensor.grid_w", 100.0, "W"),
        ("sensor.pv_w", 300.0, "W"),
        ("sensor.battery_w", 100.0, "W"),
        ("sensor.soc", 55.0, "%"),
    ]);
    assert_eq!(site.live(&charging).load_w, Some(300.0));

    // Exporting while the pack barely discharges is measurement noise: clamp, never a negative load.
    let noise = readings(&[
        ("sensor.grid_w", -200.0, "W"),
        ("sensor.pv_w", 100.0, "W"),
        ("sensor.battery_w", -50.0, "W"),
        ("sensor.soc", 55.0, "%"),
    ]);
    let live = site.live(&noise);
    assert_eq!(live.load_w, Some(0.0));
    assert!(live.load_clamped);

    let missing = readings(&[
        ("sensor.grid_w", 100.0, "W"),
        ("sensor.pv_w", 300.0, "W"),
        ("sensor.soc", 55.0, "%"),
    ]);
    assert_eq!(site.live(&missing).load_w, None);
}

#[tokio::test]
async fn not_found_prefs_means_no_dashboard() {
    let server = FakeServer::spawn(std::sync::Arc::new(|_, command, _| {
        if command == "energy/get_prefs" {
            support::Reply::fail("not_found", "No prefs")
        } else {
            support::Reply::ok(json!({}))
        }
    }))
    .await;
    let config = Config::direct(&server.base_url(), TOKEN).expect("config");
    let connection = Connection::connect(&config).await.expect("connect");

    assert!(fetch_prefs(&connection).await.expect("prefs").is_none());
    let site = discover(&connection).await.expect("discover");
    assert!(!site.configured);
    assert!(site.batteries.is_empty());
}

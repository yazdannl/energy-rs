//! Named-device subtraction parity and the add-back rules.

// Test code: a failed assertion is the report, so `unwrap`/`expect`/`panic!` are the intended API
// here, and fixture numbers read better verbatim than with digit separators.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::assert_is_empty,
    clippy::manual_midpoint,
    clippy::unreadable_literal,
    clippy::cast_possible_wrap
)]

mod support;

use std::collections::BTreeMap;

use energy_forecast::load::{
    DeviceKind, LoadDevice, LoadProfile, LoadSample, device_plan, parse_devices, split_device_load,
};
use serde_json::{Value, json};
use support::{fixture, read_json, series};

fn samples(value: &Value) -> Vec<LoadSample> {
    series(value)
        .into_iter()
        .map(|(moment, load_kw)| LoadSample { moment, load_kw })
        .collect()
}

fn device_from(value: &Value) -> LoadDevice {
    LoadDevice::new(
        value["entity"].as_str().expect("entity"),
        &DeviceKind::parse(value["kind"].as_str().expect("kind")),
        value["name"].as_str().unwrap_or_default(),
    )
}

#[test]
fn device_split_matches_the_reference() {
    let fixture = read_json(&fixture("load_devices.json"));
    let house = samples(&fixture["house"]);
    let device = device_from(&fixture["device"]);
    assert_eq!(device.key, fixture["device"]["key"].as_str().unwrap());
    assert!(!device.temperature, "an EV charger has no temperature term");
    let mut device_samples = BTreeMap::new();
    device_samples.insert(
        device.key.clone(),
        series(&fixture["device"]["series"]).into_iter().collect(),
    );
    let split = split_device_load(&house, std::slice::from_ref(&device), &device_samples);
    let expected = &fixture["expected"];
    assert_eq!(
        split.subtracted,
        vec![expected["subtracted"][0].as_str().unwrap().to_string()]
    );
    assert_eq!(
        split.dropped_hours,
        expected["dropped_hours"].as_u64().unwrap() as u32
    );
    assert_eq!(
        split.clamped_hours,
        expected["clamped_hours"].as_u64().unwrap() as u32
    );
    assert_eq!(
        split.base.len(),
        expected["base_len"].as_u64().unwrap() as usize
    );
    assert!(split.skipped.is_empty());
    let first: Vec<(String, f64)> = split
        .base
        .iter()
        .take(5)
        .map(|sample| (sample.moment.to_rfc3339(), sample.load_kw))
        .collect();
    let expected_first: Vec<(String, f64)> = expected["base_first5"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            (
                pair[0].as_str().unwrap().to_string(),
                pair[1].as_f64().unwrap(),
            )
        })
        .collect();
    assert_eq!(first, expected_first);
    let last = split.base.last().expect("base is not empty");
    let expected_last = expected["base_last"][0].as_array().unwrap();
    assert_eq!(last.moment.to_rfc3339(), expected_last[0].as_str().unwrap());
    assert_eq!(last.load_kw, expected_last[1].as_f64().unwrap());
}

#[test]
fn base_never_exceeds_the_house_total_and_is_never_negative() {
    let fixture = read_json(&fixture("load_devices.json"));
    let house = samples(&fixture["house"]);
    let house_by_stamp: BTreeMap<_, _> = house
        .iter()
        .map(|sample| (sample.moment, sample.load_kw))
        .collect();
    let device = device_from(&fixture["device"]);
    let mut device_samples = BTreeMap::new();
    device_samples.insert(
        device.key.clone(),
        series(&fixture["device"]["series"]).into_iter().collect(),
    );
    let split = split_device_load(&house, &[device], &device_samples);
    for sample in &split.base {
        assert!(sample.load_kw >= 0.0);
        let total = house_by_stamp
            .get(&sample.moment)
            .expect("base stamp is a house stamp");
        assert!(
            sample.load_kw <= *total,
            "base {} > house {total}",
            sample.load_kw
        );
    }
}

#[test]
fn a_meter_that_covers_too_little_is_refused_and_stays_in_the_base() {
    let fixture = read_json(&fixture("load_devices.json"));
    let house = samples(&fixture["house"]);
    let device = device_from(&fixture["device"]);
    let mut short: BTreeMap<_, _> = series(&fixture["device"]["series"]).into_iter().collect();
    // Keep only 60 hours, but spread them so coverage of the house window is under 50%.
    let keep: Vec<_> = short.keys().copied().take(60).collect();
    let short: BTreeMap<_, _> = keep
        .into_iter()
        .filter_map(|stamp| short.remove(&stamp).map(|value| (stamp, value)))
        .collect();
    let mut map = BTreeMap::new();
    map.insert(device.key.clone(), short);
    let split = split_device_load(&house, &[device], &map);
    assert_eq!(split.subtracted, Vec::<String>::new());
    assert_eq!(split.base.len(), house.len());
    assert!(split.skipped.contains_key("device:sensor.ev"));
}

#[test]
fn a_missing_device_hour_is_dropped_not_zero_filled() {
    let fixture = read_json(&fixture("load_devices.json"));
    let house = samples(&fixture["house"]);
    let device = device_from(&fixture["device"]);
    let complete: BTreeMap<_, _> = series(&fixture["device"]["series"]).into_iter().collect();
    let mut missing = complete.clone();
    let removed = missing.keys().next().copied().expect("one hour");
    missing.remove(&removed);
    let mut map = BTreeMap::new();
    map.insert(device.key.clone(), missing);
    let split = split_device_load(&house, &[device], &map);
    assert_eq!(split.base.len(), complete.len() - 1);
    assert!(split.base.iter().all(|sample| sample.moment != removed));
    assert_eq!(split.dropped_hours, split.dropped_hours.max(1));
}

#[test]
fn device_plan_only_adds_back_what_the_base_subtracted() {
    let mut base = LoadProfile::from_payload(&json!({
        "buckets": [],
        "devices_subtracted": ["device:sensor.ev"],
    }))
    .expect("minimal payload parses");
    base.samples = 500;
    base.days = 30;
    let ev = LoadDevice::new("sensor.ev", &DeviceKind::parse("ev_charger"), "");
    let heat_pump = LoadDevice::new("sensor.heat", &DeviceKind::parse("heat_pump"), "");
    let mut profiles = BTreeMap::new();
    let mut ev_profile = LoadProfile::from_payload(&json!({"b": 0.4, "r2": 0.6})).unwrap();
    ev_profile.kind = Some("ev_charger".to_string());
    ev_profile.fit.b = 0.4;
    ev_profile.fit.r2 = 0.6;
    profiles.insert(ev.key.clone(), ev_profile);
    let plan = device_plan(Some(&base), &[ev.clone(), heat_pump.clone()], &profiles);
    assert_eq!(plan.included.len(), 1);
    let (included, component) = &plan.included[0];
    assert_eq!(included.key, ev.key);
    // A non-temperature kind has its slope forced off at serving time.
    assert_eq!(component.fit.b, 0.0);
    assert_eq!(component.fit.r2, 0.0);
    assert_eq!(plan.excluded.len(), 1, "the heat pump was never subtracted");
    assert_eq!(plan.excluded[0].0.key, heat_pump.key);

    // No base profile: everything is excluded because the fallback already contains it.
    let plan = device_plan(None, &[ev], &profiles);
    assert_eq!(plan.included, Vec::new());
    assert_eq!(plan.excluded.len(), 1);
}

#[test]
fn parse_devices_drops_the_house_meter_duplicates_and_empty_entries() {
    let raw: Vec<Value> = vec![
        json!({"entity": "sensor.house"}),
        json!({"entity": "sensor.ev", "kind": "ev_charger", "name": "Car"}),
        json!({"entity": "sensor.ev", "kind": "ev_charger"}),
        json!({"entity": "  "}),
        json!({"entity": "sensor.other", "kind": "solar_weird"}),
    ];
    let (devices, problems) = parse_devices(&raw, "sensor.house");
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].key, "device:sensor.ev");
    assert_eq!(devices[0].name, "Car");
    assert!(
        !devices[0].temperature,
        "an EV charger is not a temperature kind"
    );
    assert_eq!(devices[1].key, "device:sensor.other");
    assert!(!devices[1].temperature);
    assert!(
        problems
            .iter()
            .any(|line| line.contains("house meter itself"))
    );
    assert!(problems.iter().any(|line| line.contains("repeats a meter")));
    assert!(problems.iter().any(|line| line.contains("names no meter")));
    assert!(problems.iter().any(|line| line.contains("unknown kind")));
}

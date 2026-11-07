//! Load-profile parity: the 48 buckets, the level, the bias and the served 15-minute slots.

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

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use energy_forecast::load::{
    Confidence, LoadInputs, LoadProfile, LoadSource, build_profile, hourly_bands, interpolated,
    load_forecast,
};
use energy_forecast::time::hour_floor;
use serde_json::Value;
use support::{fixture, read_json, series, time};

const AMSTERDAM: Tz = Tz::Europe__Amsterdam;
const WINDOW_DAYS: u32 = 56;

fn samples(value: &Value) -> Vec<energy_forecast::load::LoadSample> {
    series(value)
        .into_iter()
        .map(|(moment, load_kw)| energy_forecast::load::LoadSample { moment, load_kw })
        .collect()
}

fn temperatures(value: &Value) -> BTreeMap<DateTime<Utc>, f64> {
    series(value).into_iter().collect()
}

fn template() -> Value {
    read_json(&fixture("load_profile.json"))
}

#[test]
fn profile_learning_matches_the_reference_payload() {
    let fixture = template();
    let profile = build_profile(
        &samples(&fixture["samples"]),
        &temperatures(&fixture["temperatures"]),
        time(fixture["now"].as_str().unwrap()),
        AMSTERDAM,
        WINDOW_DAYS,
        false,
    );
    assert_buckets(&profile, &fixture["expected"]);
    assert_levels(&profile, &fixture["expected"]);
    assert_regression(&profile, &fixture["expected"]);
    assert_eq!(
        profile.samples,
        fixture["expected"]["samples"].as_u64().unwrap() as usize
    );
    assert_eq!(
        profile.days,
        fixture["expected"]["days"].as_u64().unwrap() as usize
    );
    assert_eq!(profile.samples, 1344, "56-day window truncates 1440 hours");
    assert_eq!(profile.days, 57);
    assert!(!profile.assumed_daytime_shape);
    assert_eq!(profile.updated_at, "2026-03-06T00:00:00+00:00");
}

fn assert_buckets(profile: &LoadProfile, expected: &Value) {
    let buckets = expected["buckets"].as_array().expect("buckets");
    assert_eq!(buckets.len(), 48);
    for (index, bucket) in buckets.iter().enumerate() {
        let got = profile.buckets[index];
        for key in ["median", "p10", "p90"] {
            assert_eq!(
                bucket_value(got, key).to_bits(),
                bucket[key].as_f64().unwrap().to_bits(),
                "bucket {index} {key}"
            );
        }
        assert_eq!(
            got.count,
            bucket["count"].as_u64().unwrap() as u32,
            "bucket {index} count"
        );
    }
}

fn bucket_value(bucket: energy_forecast::load::Bucket, key: &str) -> f64 {
    match key {
        "median" => bucket.median,
        "p10" => bucket.p10,
        _ => bucket.p90,
    }
}

/// The store writes `round(x, 4)`; serving reads those rounded values back, so parity is pinned
/// against the payload, not the unrounded in-memory arithmetic.
fn assert_levels(profile: &LoadProfile, expected: &Value) {
    let payload = profile.to_payload();
    let expected_bias = expected["bias"].as_array().expect("bias");
    let payload_bias = payload["bias"].as_array().expect("payload bias");
    for (index, value) in expected_bias.iter().enumerate() {
        assert_eq!(
            payload_bias[index].as_f64().unwrap().to_bits(),
            value.as_f64().unwrap().to_bits(),
            "bias {index}"
        );
    }
    for key in ["scale", "profile_mean", "recent_7d_mean"] {
        assert_eq!(
            payload[key].as_f64().unwrap().to_bits(),
            expected[key].as_f64().unwrap().to_bits(),
            "{key}"
        );
    }
}

/// The regression is stored unrounded, so compare at the precision the fixture carries.
fn assert_regression(profile: &LoadProfile, expected: &Value) {
    assert!((profile.fit.b - expected["b"].as_f64().unwrap()).abs() <= 1e-12);
    assert!((profile.fit.r2 - expected["r2"].as_f64().unwrap()).abs() <= 1e-12);
    assert!((profile.fit.t_mean.unwrap() - expected["t_mean"].as_f64().unwrap()).abs() <= 1e-12);
    assert!((profile.fit.a.unwrap() - expected["a"].as_f64().unwrap()).abs() <= 1e-12);
    assert_eq!(
        profile.temperature_hours,
        expected["temperature_hours"].as_u64().unwrap() as usize
    );
}

#[test]
fn serving_matches_the_reference_slots_exactly() {
    let fixture = template();
    let profile = LoadProfile::from_payload(&fixture["expected"]).expect("payload parses");
    let serving = &fixture["serving"];
    let first_hour = time(serving["first_hour"].as_str().unwrap());
    let last_hour = time(serving["last_hour"].as_str().unwrap());
    let temperatures = temperatures(&serving["temperature_c"]);
    let bands = hourly_bands(
        Some(&profile),
        first_hour,
        last_hour,
        AMSTERDAM,
        serving["base_kwh_per_day"].as_f64().unwrap(),
        &temperatures,
    );
    let slots = serving["slots"].as_array().expect("slots");
    assert_eq!(slots.len(), 96);
    for (index, slot) in slots.iter().enumerate() {
        let at = time(slot["t"].as_str().unwrap());
        let hour = hour_floor(at);
        let p50 = interpolated(&bands.band, hour, at);
        let p10 = interpolated(&bands.low, hour, at).max(0.0);
        let p90 = interpolated(&bands.high, hour, at).max(0.0);
        assert_eq!(
            p50.to_bits(),
            slot["p50"].as_f64().unwrap().to_bits(),
            "slot {index} p50"
        );
        assert_eq!(
            p10.to_bits(),
            slot["p10"].as_f64().unwrap().to_bits(),
            "slot {index} p10"
        );
        assert_eq!(
            p90.to_bits(),
            slot["p90"].as_f64().unwrap().to_bits(),
            "slot {index} p90"
        );
    }
}

#[test]
fn load_forecast_serves_the_profile_and_its_bands() {
    let fixture = template();
    let profile = LoadProfile::from_payload(&fixture["expected"]).expect("payload parses");
    let now = time("2026-03-06T00:00:00+00:00");
    let temperatures = temperatures(&fixture["serving"]["temperature_c"]);
    let forecast = load_forecast(
        Some(&profile),
        &[],
        &LoadInputs::new(
            &BTreeMap::new(),
            &temperatures,
            20.0,
            now,
            AMSTERDAM,
            Some(24),
        ),
    );
    assert_eq!(forecast.slots.len(), 96);
    assert_eq!(forecast.source, LoadSource::Profile);
    assert_eq!(forecast.source.as_str(), "profile");
    assert_eq!(forecast.confidence, Confidence::High);
    for slot in &forecast.slots {
        assert!(slot.load_kw >= 0.0);
        assert!(slot.p10_kw <= slot.load_kw + 1e-9, "p10 <= p50");
        assert!(slot.load_kw <= slot.p90_kw + 1e-9, "p50 <= p90");
    }
    assert_eq!(forecast.notes, Vec::<String>::new());
}

#[test]
fn fallback_and_confidence_classes() {
    let now = time("2026-03-06T00:00:00+00:00");
    let forecast = load_forecast(
        None,
        &[],
        &LoadInputs::new(
            &BTreeMap::new(),
            &BTreeMap::new(),
            20.0,
            now,
            AMSTERDAM,
            None,
        ),
    );
    assert_eq!(forecast.source, LoadSource::Fallback);
    assert_eq!(forecast.confidence, Confidence::Low);
    assert_eq!(forecast.slots.len(), 36 * 4);
    assert!(
        forecast
            .notes
            .iter()
            .any(|note| note.contains("cold start"))
    );
    let empty = load_forecast(
        None,
        &[],
        &LoadInputs::new(
            &BTreeMap::new(),
            &BTreeMap::new(),
            0.0,
            now,
            AMSTERDAM,
            None,
        ),
    );
    assert_eq!(empty.source, LoadSource::FallbackEmpty);
    assert!(empty.slots.iter().all(|slot| slot.load_kw == 0.0));
}

#[test]
fn dst_days_still_serve_a_full_quarter_hour_grid() {
    let profile = LoadProfile::from_payload(&template()["expected"]).expect("payload parses");
    for start in ["2026-03-29T00:00:00+00:00", "2026-10-25T00:00:00+00:00"] {
        let forecast = load_forecast(
            Some(&profile),
            &[],
            &LoadInputs::new(
                &BTreeMap::new(),
                &BTreeMap::new(),
                20.0,
                time(start),
                AMSTERDAM,
                Some(24),
            ),
        );
        assert_eq!(forecast.slots.len(), 96, "{start}");
        for slot in &forecast.slots {
            assert!(slot.load_kw.is_finite() && slot.load_kw >= 0.0);
        }
    }
}

#[test]
fn night_fill_keeps_the_observed_level() {
    // Six night hours observed at 1 kW; the fill must give a day that still averages 1 kW over the
    // observed buckets' level rather than inflating every hour.
    let now = time("2026-03-06T00:00:00+00:00");
    let mut samples = Vec::new();
    for day in 0..10 {
        for hour in [0, 1, 2, 3, 4, 5] {
            samples.push(energy_forecast::load::LoadSample {
                moment: now - chrono::Duration::days(day) + chrono::Duration::hours(hour),
                load_kw: 1.0,
            });
        }
    }
    let profile = build_profile(
        &samples,
        &BTreeMap::new(),
        now,
        AMSTERDAM,
        WINDOW_DAYS,
        true,
    );
    assert!(profile.assumed_daytime_shape);
    assert!(profile.buckets.iter().all(|bucket| bucket.median > 0.0));
    let observed_level = 1.0;
    let mean: f64 = profile
        .buckets
        .iter()
        .map(|bucket| bucket.median)
        .sum::<f64>()
        / 48.0;
    assert!(
        (mean - observed_level).abs() < 1e-3,
        "filled mean {mean} should stay at the observed level"
    );
}

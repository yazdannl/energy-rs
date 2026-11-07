//! Determinism: the same inputs produce the same bits, and payloads survive a round trip.

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

use chrono::Duration;
use chrono_tz::Tz;
use energy_forecast::bias::{HeadProfile, HourlyBias, evaluate, fit_factors};
use energy_forecast::features::{FEATURE_NAMES, Features};
use energy_forecast::load::{LoadProfile, LoadSample, build_profile};
use energy_forecast::physics::Site;
use energy_forecast::pv::{SlotInputs, estimate_slot};
use support::{fixture, json_lines, model, numbers, read_json, series, time};

const AMSTERDAM: Tz = Tz::Europe__Amsterdam;

#[test]
fn the_golden_walk_is_deterministic_and_reload_stable() {
    let first = model();
    let second = model();
    for value in json_lines("pv_golden.jsonl") {
        let row = Features::from_slice(&numbers(&value["features"])).unwrap();
        let a = first.ratio(&row).unwrap();
        let b = first.ratio(&row).unwrap();
        let c = second.ratio(&row).unwrap();
        assert_eq!(a.to_bits(), b.to_bits());
        assert_eq!(a.to_bits(), c.to_bits());
    }
}

#[test]
fn estimate_slots_are_byte_identical_across_runs() {
    let model = model();
    let at = time("2026-06-21T12:00:00+00:00");
    let mut inputs = SlotInputs::new(
        at,
        Site::new(52.0, 5.0, 35.0, 180.0),
        4000.0,
        4000.0,
        640.0,
        18.0,
    );
    inputs.cloud_pct = 40.0;
    inputs.attenuation = 0.9;
    let first = estimate_slot(&model, &inputs).unwrap();
    let second = estimate_slot(&model, &inputs).unwrap();
    assert_eq!(first.pv_kw.to_bits(), second.pv_kw.to_bits());
    assert_eq!(first.ratio.to_bits(), second.ratio.to_bits());
    assert_eq!(first.physics_w.to_bits(), second.physics_w.to_bits());
    assert_eq!(first.poa_wm2.to_bits(), second.poa_wm2.to_bits());
    assert_eq!(first.t_cell_c.to_bits(), second.t_cell_c.to_bits());
    assert_eq!(
        first.elevation_deg.to_bits(),
        second.elevation_deg.to_bits()
    );
}

#[test]
fn profile_payload_round_trips_without_drift() {
    let fixture = read_json(&fixture("load_profile.json"));
    let samples: Vec<LoadSample> = series(&fixture["samples"])
        .into_iter()
        .map(|(moment, load_kw)| LoadSample { moment, load_kw })
        .collect();
    let temperatures: BTreeMap<_, _> = series(&fixture["temperatures"]).into_iter().collect();
    let profile = build_profile(
        &samples,
        &temperatures,
        time("2026-03-06T00:00:00+00:00"),
        AMSTERDAM,
        56,
        false,
    );
    let payload = profile.to_payload();
    let reloaded = LoadProfile::from_payload(&payload).expect("payload parses");
    assert_eq!(reloaded.to_payload(), payload);
    assert_eq!(reloaded, profile);
}

#[test]
fn bias_and_head_payloads_round_trip() {
    let mut bias = HourlyBias::neutral();
    bias.factors[7] = 1.234_567;
    bias.counts[7] = 3;
    bias.mape_pct = Some(12.345);
    bias.updated_at = Some(time("2026-03-06T00:00:00+00:00"));
    let payload = bias.to_payload();
    let reloaded = HourlyBias::from_payload(&payload).expect("payload parses");
    assert_eq!(reloaded.to_payload(), payload);

    let samples: Vec<_> = (0..40)
        .map(|index| energy_forecast::bias::HeadSample {
            moment: time("2026-03-01T00:00:00+00:00") + Duration::hours(index),
            hour: u32::try_from(index).unwrap_or(0) % 24,
            forecast_kw: 1.0 + (index as f64) * 0.1,
            actual_kw: 0.9 + (index as f64) * 0.12,
        })
        .collect();
    let (fit, holdout) = samples.split_at(30);
    let report = evaluate(fit, holdout, &[1.0; 24]);
    let factors = fit_factors(fit, 25.0);
    let previous = HeadProfile::from_report(
        &energy_forecast::bias::HeadReport {
            status: energy_forecast::bias::HeadStatus::Rejected,
            promoted: false,
            samples: 1,
            fit_samples: 1,
            holdout_samples: 0,
            candidate_mae_kw: None,
            incumbent_mae_kw: None,
            reason: "seed".to_string(),
        },
        [1.0; 24],
        "solar",
        time("2026-02-01T00:00:00+00:00"),
        None,
    );
    let profile = HeadProfile::from_report(
        &report,
        factors,
        "solar",
        time("2026-03-02T00:00:00+00:00"),
        Some(previous),
    );
    let payload = profile.to_payload();
    let reloaded = HeadProfile::from_payload(&payload).expect("payload parses");
    assert_eq!(reloaded.to_payload(), payload);
    assert_eq!(reloaded.active_factors(), profile.active_factors());
    // Reverting restores the previous payload.
    let mut reverted = reloaded.clone();
    assert!(reverted.revert(time("2026-03-03T00:00:00+00:00")));
    assert!(!reverted.promoted);
    assert!(reverted.previous.is_none());
    assert_eq!(reverted.status, "reverted");
}

#[test]
fn no_feature_order_drift_in_the_payload_round_trip() {
    // A structural guard: the contract's length and order are what every payload above assumes.
    assert_eq!(FEATURE_NAMES.len(), 28);
    assert_eq!(
        FEATURE_NAMES.last().copied(),
        Some("rain_mm"),
        "adding a feature at the end is still a contract change"
    );
}

//! The hard gate: 800 golden vectors, bit for bit, plus the artifact guards around them.

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

use energy_forecast::ForecastError;
use energy_forecast::features::{FEATURE_COUNT, Features, index};
use energy_forecast::model::{PvModel, feature_digest};
use energy_forecast::physics::{Site, ac_power};
use energy_forecast::pv::{SlotInputs, estimate_slot, slot_features};
use serde_json::Value;
use support::{artifact_json, json_lines, model, numbers};

#[test]
fn golden_ratio_is_exact_for_all_800_vectors() {
    let model = model();
    let cases = json_lines("pv_golden.jsonl");
    assert_eq!(cases.len(), 800);
    for (case, value) in cases.iter().enumerate() {
        let features = Features::from_slice(&numbers(&value["features"])).expect("28 features");
        let expected = value["expected"].as_f64().expect("expected ratio");
        let ratio = model.ratio(&features).expect("walk succeeds");
        assert_eq!(
            ratio.to_bits(),
            expected.to_bits(),
            "case {case}: ratio {ratio} != expected {expected}"
        );
    }
}

#[test]
fn golden_raw_ratio_and_physics_are_exact() {
    let model = model();
    let cases = json_lines("pv_golden_full.jsonl");
    let mut night = 0;
    let mut clamped = 0;
    for (case, value) in cases.iter().enumerate() {
        let features = numbers(&value["features"]);
        let expected_raw = value["raw_ratio"].as_f64().expect("raw ratio");
        let expected_physics = value["physics_w"].as_f64().expect("physics_w");
        let expected_clamped = value["expected"].as_f64().expect("clamped ratio");
        let row = Features::from_slice(&features).expect("28 features");
        let raw = model.raw_ratio(&row).expect("walk succeeds");
        assert_eq!(
            raw.to_bits(),
            expected_raw.to_bits(),
            "raw ratio, case {case}"
        );
        // The physics the artifact was trained against is reproducible from the features alone.
        let physics = ac_power(
            features[14] * 1000.0,
            features[15] * 1000.0,
            features[8],
            features[13],
        );
        assert_eq!(
            physics.to_bits(),
            expected_physics.to_bits(),
            "physics_w, case {case}"
        );
        let clamped_ratio = model.ratio(&row).expect("walk succeeds");
        assert_eq!(
            clamped_ratio.to_bits(),
            expected_clamped.to_bits(),
            "clamped ratio, case {case}"
        );
        if value["elevation_deg"].as_f64().expect("elevation") <= 0.0 {
            night += 1;
        }
        if expected_raw.to_bits() != expected_clamped.to_bits() {
            clamped += 1;
        }
    }
    assert_eq!(
        night, 420,
        "the fixture's night-case count is part of the contract"
    );
    assert_eq!(clamped, 212, "212 cases clamp to the lower bound");
}

#[test]
fn golden_features_rebuild_from_inputs_and_still_walk_exactly() {
    let model = model();
    let cases = json_lines("pv_features.jsonl");
    assert_eq!(cases.len(), 800);
    let mut checked = 0;
    for (case, value) in cases.iter().enumerate() {
        let inputs = slot_inputs(value);
        let expected_features = numbers(&value["features"]);
        let Some((built, physics_w, poa_wm2, elevation)) = slot_features(&inputs) else {
            // Night: the serving path builds no features at all.
            assert!(
                expected_features[index::ELEVATION_DEG] <= 0.0,
                "case {case}"
            );
            let estimate = estimate_slot(&model, &inputs).expect("night estimate");
            assert_eq!(estimate.pv_kw, 0.0, "night pv_kw, case {case}");
            assert_eq!(estimate.ratio, 0.0, "night ratio, case {case}");
            continue;
        };
        assert!(elevation > 0.0);
        assert_features(&built, &expected_features, case);
        assert_eq!(
            physics_w.to_bits(),
            value["physics_w"].as_f64().unwrap().to_bits()
        );
        assert_eq!(
            poa_wm2.to_bits(),
            value["poa_wm2"].as_f64().unwrap().to_bits()
        );
        // The recomputed vector must still reach the same leaf set as the Python walk.
        let ratio = model.ratio(&built).expect("walk succeeds");
        assert_eq!(
            ratio.to_bits(),
            value["clamped_ratio"].as_f64().unwrap().to_bits(),
            "recomputed-feature ratio, case {case}"
        );
        let estimate = estimate_slot(&model, &inputs).expect("estimate");
        assert_eq!(
            estimate.pv_kw.to_bits(),
            value["pv_kw"].as_f64().unwrap().to_bits(),
            "estimate pv_kw, case {case}"
        );
        checked += 1;
    }
    assert_eq!(checked, 380, "380 daytime cases build features and walk");
}

fn assert_features(built: &Features, expected: &[f64], case: usize) {
    for (column, want) in expected.iter().enumerate().take(FEATURE_COUNT) {
        let got = built.get(column);
        let tolerance = 1e-9 * want.abs().max(1.0);
        assert!(
            (got - want).abs() <= tolerance,
            "case {case} feature {column}: {got} != {want}"
        );
    }
}

fn slot_inputs(value: &Value) -> SlotInputs {
    let input = &value["in"];
    SlotInputs {
        moment: time_iso(input["epoch"].as_i64().expect("epoch")),
        site: Site::new(
            number(input, "lat"),
            number(input, "lon"),
            number(input, "tilt"),
            number(input, "azimuth"),
        ),
        dc_capacity_wp: number(input, "dc_wp"),
        ac_capacity_wp: number(input, "ac_wp"),
        ghi_wm2: number(input, "ghi_wm2"),
        t_ambient_c: number(input, "t_ambient_c"),
        cloud_pct: number(input, "cloud_pct"),
        attenuation: number(input, "attenuation"),
        wind_ms: number(input, "wind_ms"),
        rh_pct: number(input, "rh_pct"),
        rain_mm: number(input, "rain_mm"),
    }
}

#[test]
fn artifact_shape_and_metadata_are_the_committed_ones() {
    let model = model();
    assert_eq!(model.tree_count(), 400);
    assert_eq!(model.node_count(), 23_336);
    assert_eq!(model.leaf_count(), 11_868);
    assert_eq!(model.clamp(), (0.3, 1.8));
    let metadata = model.metadata();
    assert_eq!(metadata.feature_digest.as_deref(), Some("028ba35d4f44d148"));
    assert_eq!(metadata.training_rows, Some(1_885_671));
    assert_eq!(metadata.training_systems, Some(174));
    assert_eq!(metadata.trained_at.as_deref(), Some("2026-10-14T17:24:32Z"));
    assert_eq!(feature_digest(), "028ba35d4f44d148");
}

#[test]
fn refuses_wrong_kind() {
    let mut artifact = artifact_json();
    artifact["kind"] = Value::String("something_else".to_string());
    let error = PvModel::from_json(&serde_json::to_vec(&artifact).unwrap()).unwrap_err();
    assert!(matches!(error, ForecastError::BadArtifact(_)), "{error:?}");
}

#[test]
fn refuses_reordered_feature_names() {
    let mut artifact = artifact_json();
    let names = artifact["feature_names"].as_array_mut().unwrap();
    names.swap(0, 1);
    let error = PvModel::from_json(&serde_json::to_vec(&artifact).unwrap()).unwrap_err();
    assert!(
        matches!(error, ForecastError::FeatureContract(_)),
        "{error:?}"
    );
}

#[test]
fn refuses_wrong_feature_digest() {
    let mut artifact = artifact_json();
    artifact["feature_digest"] = Value::String("0000000000000000".to_string());
    let error = PvModel::from_json(&serde_json::to_vec(&artifact).unwrap()).unwrap_err();
    assert!(
        matches!(error, ForecastError::FeatureContract(_)),
        "{error:?}"
    );
}

#[test]
fn refuses_truncated_tree() {
    let mut artifact = artifact_json();
    let first = artifact["trees"][0].as_object_mut().unwrap();
    let values = first.get_mut("value_bits").unwrap().as_array_mut().unwrap();
    values.pop();
    let error = PvModel::from_json(&serde_json::to_vec(&artifact).unwrap()).unwrap_err();
    assert!(matches!(error, ForecastError::BadArtifact(_)), "{error:?}");
}

#[test]
fn refuses_out_of_range_child_index() {
    let mut artifact = artifact_json();
    let first = artifact["trees"][0].as_object_mut().unwrap();
    let left = first.get_mut("left").unwrap().as_array_mut().unwrap();
    left[0] = Value::from(9999);
    let error = PvModel::from_json(&serde_json::to_vec(&artifact).unwrap()).unwrap_err();
    assert!(matches!(error, ForecastError::BadArtifact(_)), "{error:?}");
}

#[test]
fn refuses_identical_children() {
    let mut artifact = artifact_json();
    let first = artifact["trees"][0].as_object_mut().unwrap();
    let right_value = first.get("right").unwrap().as_array().unwrap()[0].clone();
    first.get_mut("left").unwrap().as_array_mut().unwrap()[0] = right_value;
    let error = PvModel::from_json(&serde_json::to_vec(&artifact).unwrap()).unwrap_err();
    assert!(matches!(error, ForecastError::BadArtifact(_)), "{error:?}");
}

#[test]
fn refuses_unusable_clamp() {
    let mut artifact = artifact_json();
    artifact["residual_clamp"] = serde_json::json!([1.8, 0.3]);
    let error = PvModel::from_json(&serde_json::to_vec(&artifact).unwrap()).unwrap_err();
    assert!(matches!(error, ForecastError::BadArtifact(_)), "{error:?}");
}

#[test]
fn missing_artifact_is_reported_not_panicked() {
    let error =
        PvModel::load_path(&support::crate_dir().join("models/does-not-exist.json")).unwrap_err();
    assert!(
        matches!(error, ForecastError::ModelUnavailable(_)),
        "{error:?}"
    );
}

#[test]
fn raw_walk_leaves_the_clamp_and_the_served_ratio_does_not() {
    let model = model();
    let (low, high) = model.clamp();
    let mut escaped = 0;
    for value in json_lines("pv_golden_full.jsonl") {
        let row = Features::from_slice(&numbers(&value["features"])).unwrap();
        let raw = model.raw_ratio(&row).unwrap();
        let ratio = model.ratio(&row).unwrap();
        assert!((low..=high).contains(&ratio));
        if raw < low || raw > high {
            escaped += 1;
        }
    }
    assert_eq!(escaped, 212);
}

fn number(input: &Value, key: &str) -> f64 {
    input[key].as_f64().expect("number")
}

fn time_iso(epoch: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(epoch, 0).expect("epoch is valid")
}

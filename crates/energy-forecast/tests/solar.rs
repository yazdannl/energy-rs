//! Solar source parity: merge, interpolation, HA Energy, EWMA bias, head fit and derived size.

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
use energy_forecast::bias::{
    BiasSample, HeadSample, HourlyBias, evaluate, fit_factors, mean_absolute_error,
};
use energy_forecast::physics::{CapacityConfig, Site};
use energy_forecast::solar::{
    MergeStrategy, Series, SolarSource, SourceHealth, SourceKind, SourceState,
    ha_energy_power_series, implied_capacity_wp, merge_solar_series, parse_ha_energy_wh,
};
use serde_json::Value;
use support::{fixture, read_json, series, time};

const AMSTERDAM: Tz = Tz::Europe__Amsterdam;

fn state(text: &str) -> SourceState {
    match text {
        "ok" => SourceState::Ok,
        "degraded" => SourceState::Degraded,
        "failed" => SourceState::Failed,
        "disabled" => SourceState::Disabled,
        other => panic!("unknown state {other}"),
    }
}

fn merge_fixture() -> Value {
    read_json(&fixture("solar_pure.json"))
}

#[test]
fn merge_matches_the_reference_for_every_strategy() {
    let fixture = merge_fixture();
    let block = &fixture["merge"];
    let grid: Vec<DateTime<Utc>> = block["grid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| time(value.as_str().unwrap()))
        .collect();
    let sources: Vec<SolarSource> = block["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            SolarSource::new(
                entry["name"].as_str().unwrap(),
                SourceKind::ForecastSolar,
                entry["weight"].as_f64().unwrap(),
                state(entry["state"].as_str().unwrap()),
                Series::new(series(&entry["series"])),
            )
        })
        .collect();
    for (label, strategy) in [
        ("weighted_mean", MergeStrategy::WeightedMean),
        ("median", MergeStrategy::Median),
        ("max", MergeStrategy::Max),
    ] {
        assert_eq!(strategy.as_str(), label);
        let outcome = merge_solar_series(&grid, &sources, strategy);
        let expected = &block["results"][label];
        let expected_values = expected["values"].as_array().unwrap();
        assert_eq!(outcome.values.len(), expected_values.len());
        for (index, value) in expected_values.iter().enumerate() {
            assert_eq!(
                outcome.values[index].to_bits(),
                value.as_f64().unwrap().to_bits(),
                "{label} slot {index}"
            );
        }
        let expected_rows = expected["report"].as_array().unwrap();
        assert_eq!(outcome.report.len(), expected_rows.len());
        for (index, row) in expected_rows.iter().enumerate() {
            let got = &outcome.report[index];
            assert_eq!(got.name, row["name"].as_str().unwrap());
            assert_eq!(got.state, state(row["state"].as_str().unwrap()));
            assert_eq!(got.points, row["points"].as_u64().unwrap() as usize);
            assert_eq!(got.weight, row["weight"].as_f64().unwrap());
        }
    }
}

#[test]
fn a_failed_source_with_a_non_empty_series_still_contributes() {
    let at = time("2026-03-06T06:00:00+00:00");
    let grid = vec![at];
    let ok = SolarSource::new(
        "ok",
        SourceKind::ForecastSolar,
        1.0,
        SourceState::Ok,
        Series::new(vec![(at, 2.0)]),
    );
    let failed = SolarSource::new(
        "failed",
        SourceKind::ForecastSolar,
        1.0,
        SourceState::Failed,
        Series::new(vec![(at, 4.0)]),
    );
    let outcome = merge_solar_series(&grid, &[ok, failed], MergeStrategy::WeightedMean);
    assert_eq!(
        outcome.values[0], 3.0,
        "a failed provider's numbers are used"
    );
    assert_eq!(outcome.report[1].state, SourceState::Failed);
}

#[test]
fn series_interpolation_has_the_provider_edge_rules() {
    let start = time("2026-03-06T06:00:00+00:00");
    let series = Series::new(vec![
        (start, 1.0),
        (start + chrono::Duration::hours(2), 3.0),
    ]);
    assert_eq!(series.interpolate(start - chrono::Duration::hours(1)), 0.0);
    assert_eq!(series.interpolate(start), 1.0);
    assert_eq!(series.interpolate(start + chrono::Duration::hours(1)), 2.0);
    assert_eq!(series.interpolate(start + chrono::Duration::hours(2)), 3.0);
    assert_eq!(
        series.interpolate(start + chrono::Duration::hours(9)),
        3.0,
        "after the last sample the series holds its last value"
    );
    assert_eq!(Series::default().interpolate(start), 0.0);
}

#[test]
fn merge_helpers_are_the_documented_ones() {
    let at = time("2026-03-06T06:00:00+00:00");
    let grid = vec![at, at + chrono::Duration::hours(1)];
    let only = SolarSource::new(
        "solo",
        SourceKind::Capacity,
        3.0,
        SourceState::Ok,
        Series::new(vec![(at, 1.25), (at + chrono::Duration::hours(2), 2.5)]),
    );
    let outcome = merge_solar_series(
        &grid,
        std::slice::from_ref(&only),
        MergeStrategy::WeightedMean,
    );
    for (index, slot) in grid.iter().enumerate() {
        assert_eq!(outcome.values[index], only.series.interpolate(*slot));
    }
    // Equal weights give the arithmetic mean.
    let a = SolarSource::new(
        "a",
        SourceKind::ForecastSolar,
        1.0,
        SourceState::Ok,
        Series::new(vec![(at, 1.0)]),
    );
    let b = SolarSource::new(
        "b",
        SourceKind::HaEnergy,
        1.0,
        SourceState::Ok,
        Series::new(vec![(at, 3.0)]),
    );
    let outcome = merge_solar_series(&grid, &[a, b], MergeStrategy::WeightedMean);
    assert_eq!(outcome.values[0], 2.0);
    assert_eq!(
        MergeStrategy::parse("nonsense"),
        MergeStrategy::WeightedMean
    );
    assert_eq!(MergeStrategy::parse("MAX"), MergeStrategy::Max);
    // A disabled source never contributes.
    let off = SolarSource::new(
        "off",
        SourceKind::ForecastSolar,
        1.0,
        SourceState::Disabled,
        Series::new(vec![(at, 99.0)]),
    );
    let outcome = merge_solar_series(&grid, &[off], MergeStrategy::Max);
    assert!(outcome.values.iter().all(|value| *value == 0.0));
    assert_eq!(outcome.report[0].state, SourceState::Disabled);
}

#[test]
fn ha_energy_converts_wh_to_kw_with_period_clamps() {
    let fixture = merge_fixture();
    let payload = &fixture["ha_energy_payload"];
    for (selection, expected) in fixture["ha_energy"].as_object().unwrap() {
        let entry = if selection == "all" {
            ""
        } else {
            selection.as_str()
        };
        let points = parse_ha_energy_wh(payload, entry);
        assert_eq!(
            points.providers,
            expected["providers"].as_u64().unwrap() as usize,
            "providers for {selection}"
        );
        let expected_wh = expected["wh"].as_array().unwrap();
        assert_eq!(points.wh_by_stamp.len(), expected_wh.len());
        for pair in expected_wh {
            let stamp = time(pair[0].as_str().unwrap());
            let value = pair[1].as_f64().unwrap();
            assert_eq!(
                points.wh_by_stamp.get(&stamp).copied().unwrap().to_bits(),
                value.to_bits()
            );
        }
        let power = ha_energy_power_series(&points.wh_by_stamp);
        let expected_series = series(&expected["series"]);
        assert_eq!(power.samples().len(), expected_series.len());
        for (index, (stamp, value)) in expected_series.iter().enumerate() {
            let got = power.samples()[index];
            assert_eq!(got.0, *stamp);
            assert_eq!(
                got.1.to_bits(),
                value.to_bits(),
                "{selection} point {index}"
            );
        }
    }
    let missing = parse_ha_energy_wh(payload, "no-such-provider");
    assert!(missing.wh_by_stamp.is_empty());
    assert_eq!(missing.providers, 0);
    // A period longer than three hours is clamped, so a sparse point cannot fake a spike.
    let mut sparse = BTreeMap::new();
    sparse.insert(time("2026-03-06T06:00:00+00:00"), 3000.0);
    sparse.insert(time("2026-03-06T18:00:00+00:00"), 1000.0);
    let power = ha_energy_power_series(&sparse);
    assert_eq!(
        power.samples()[0].1,
        1.0,
        "3000 Wh over a clamped 3 h is 1 kW"
    );
    assert_eq!(power.samples()[1].1, 1.0);
}

#[test]
fn bias_update_matches_the_reference_payload() {
    let fixture = merge_fixture();
    let block = &fixture["bias"];
    let now = time(block["now"].as_str().unwrap());
    let samples: Vec<BiasSample> = block["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| BiasSample {
            moment: time(row[0].as_str().unwrap()),
            forecast_kw: row[1].as_f64().unwrap(),
            measured_kw: row[2].as_f64().unwrap(),
        })
        .collect();
    let mut bias = HourlyBias::neutral();
    bias.update(&samples, now, AMSTERDAM);
    let expected = &block["payload"];
    let payload = bias.to_payload();
    let expected_factors = expected["factors"].as_array().unwrap();
    let actual_factors = payload["factors"].as_array().unwrap();
    for (index, value) in expected_factors.iter().enumerate() {
        assert_eq!(
            actual_factors[index].as_f64().unwrap().to_bits(),
            value.as_f64().unwrap().to_bits(),
            "factor {index}"
        );
    }
    assert_eq!(
        payload["counts"].as_array().unwrap().len(),
        expected["counts"].as_array().unwrap().len()
    );
    for (index, value) in expected["counts"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            payload["counts"][index].as_u64().unwrap(),
            value.as_u64().unwrap(),
            "count {index}"
        );
    }
    assert_eq!(
        payload["mape_pct"].as_f64().unwrap().to_bits(),
        expected["mape_pct"].as_f64().unwrap().to_bits()
    );
    assert_eq!(
        payload["updated_at"].as_str().unwrap(),
        expected["updated_at"].as_str().unwrap()
    );
}

#[test]
fn bias_decay_matches_the_reference() {
    let fixture = merge_fixture();
    let block = &fixture["bias"];
    let now = time(block["now"].as_str().unwrap());
    for case in block["decay"].as_array().unwrap() {
        let label = case["label"].as_str().unwrap();
        let factors: Vec<f64> = case["factors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_f64().unwrap())
            .collect();
        let array: [f64; 24] = factors.clone().try_into().expect("24 factors");
        let mut bias = HourlyBias::neutral();
        bias.factors = array;
        bias.updated_at = case["updated_at"]
            .as_str()
            .and_then(energy_forecast::time::parse_iso_utc);
        let decayed = bias.factors_at(now);
        let expected = case["decayed"].as_array().unwrap();
        for (index, value) in expected.iter().enumerate() {
            assert_eq!(
                decayed[index].to_bits(),
                value.as_f64().unwrap().to_bits(),
                "{label} factor {index}"
            );
        }
    }
}

#[test]
fn head_fit_and_gate_match_the_reference() {
    let fixture = merge_fixture();
    let block = &fixture["head"];
    let to_samples = |value: &Value| -> Vec<HeadSample> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|row| HeadSample {
                moment: time(row["moment"].as_str().unwrap()),
                hour: row["hour"].as_u64().unwrap() as u32,
                forecast_kw: row["forecast_kw"].as_f64().unwrap(),
                actual_kw: row["actual_kw"].as_f64().unwrap(),
            })
            .collect()
    };
    let fit = to_samples(&block["fit"]);
    let holdout = to_samples(&block["holdout"]);
    let candidate = fit_factors(&fit, 25.0);
    let expected_factors = block["candidate_factors"].as_array().unwrap();
    for (index, value) in expected_factors.iter().enumerate() {
        assert_eq!(
            candidate[index].to_bits(),
            value.as_f64().unwrap().to_bits(),
            "factor {index}"
        );
    }
    let incumbent: [f64; 24] = block["incumbent_factors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_f64().unwrap())
        .collect::<Vec<f64>>()
        .try_into()
        .expect("24 factors");
    assert_eq!(
        mean_absolute_error(&holdout, &candidate).unwrap().to_bits(),
        block["candidate_mae"].as_f64().unwrap().to_bits()
    );
    assert_eq!(
        mean_absolute_error(&holdout, &incumbent).unwrap().to_bits(),
        block["incumbent_mae"].as_f64().unwrap().to_bits()
    );
    let report = evaluate(&fit, &holdout, &incumbent);
    assert!(report.promoted, "candidate beats the incumbent by >2%");
    assert_eq!(report.status.as_str(), "promoted");
    assert_eq!(
        report.candidate_mae_kw.unwrap().to_bits(),
        block["candidate_mae"].as_f64().unwrap().to_bits()
    );
    // Too little evidence: nothing is promoted.
    let thin = evaluate(&fit[..10], &holdout, &incumbent);
    assert_eq!(thin.status.as_str(), "insufficient");
    assert!(!thin.promoted);
}

#[test]
fn derived_capacity_matches_the_reference() {
    let fixture = merge_fixture();
    let derived = &fixture["derived"];
    let config = CapacityConfig {
        capacity_wp: 4000.0,
        performance_ratio: derived["performance_ratio"].as_f64().unwrap(),
    };
    let site = Site::new(
        derived["lat"].as_f64().unwrap(),
        derived["lon"].as_f64().unwrap(),
        derived["tilt_deg"].as_f64().unwrap(),
        derived["azimuth_deg"].as_f64().unwrap(),
    );
    let fitted = implied_capacity_wp(
        &site,
        &series(&derived["samples"]),
        &config,
        None,
        AMSTERDAM,
    );
    assert_eq!(fitted.samples, derived["usable"].as_u64().unwrap() as usize);
    assert_eq!(
        fitted.capacity_wp.to_bits(),
        derived["capacity_wp"].as_f64().unwrap().to_bits()
    );
}

#[test]
fn source_states_are_classified() {
    assert_eq!(
        SourceState::classify(SourceHealth::Inactive, false),
        SourceState::Disabled
    );
    assert_eq!(
        SourceState::classify(SourceHealth::Failed, true),
        SourceState::Failed
    );
    assert_eq!(
        SourceState::classify(SourceHealth::Ok, false),
        SourceState::Failed
    );
    assert_eq!(
        SourceState::classify(SourceHealth::Degraded, true),
        SourceState::Degraded
    );
    assert_eq!(
        SourceState::classify(SourceHealth::Ok, true),
        SourceState::Ok
    );
}

#[test]
fn source_kinds_are_a_closed_set() {
    for name in [
        "forecast_solar",
        "ha_energy",
        "capacity",
        "derived",
        "learned",
    ] {
        let kind = SourceKind::parse(name).expect("known kind");
        assert_eq!(kind.as_str(), name);
    }
    assert!(SourceKind::parse("open_meteo").is_none());
}

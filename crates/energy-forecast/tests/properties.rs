//! Property tests: the invariants that must hold for arbitrary inputs, not just the fixtures.

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
use std::sync::OnceLock;

use chrono::Duration;
use chrono_tz::Tz;
use energy_forecast::bias::decay;
use energy_forecast::features::Features;
use energy_forecast::load::{
    LoadDevice, LoadInputs, LoadSample, LoadSource, build_profile, quantile, split_device_load,
};
use energy_forecast::model::PvModel;
use energy_forecast::physics::{CapacityConfig, Site};
use energy_forecast::pv::{SlotInputs, estimate_slot};
use energy_forecast::solar::{
    MergeStrategy, Series, SolarSource, SourceKind, SourceState, merge_solar_series,
};
use proptest::prelude::*;
use support::{model, time};

const AMSTERDAM: Tz = Tz::Europe__Amsterdam;

fn model_ref() -> &'static PvModel {
    static MODEL: OnceLock<PvModel> = OnceLock::new();
    MODEL.get_or_init(model)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn the_served_ratio_is_always_inside_the_clamp(values in prop::collection::vec(any::<f64>(), 28)) {
        let row = Features::from_slice(&values).expect("28 values");
        let ratio = model_ref().ratio(&row).expect("walk terminates");
        let (low, high) = model_ref().clamp();
        prop_assert!(ratio.is_finite(), "ratio {ratio} is not finite");
        prop_assert!((low..=high).contains(&ratio), "ratio {ratio} outside [{low}, {high}]");
    }

    #[test]
    fn quantiles_are_monotone(values in prop::collection::vec(-1e6..1e6f64, 1..200)) {
        let p10 = quantile(&values, 0.1);
        let p50 = quantile(&values, 0.5);
        let p90 = quantile(&values, 0.9);
        prop_assert!(p10 <= p50 && p50 <= p90, "{p10} {p50} {p90}");
    }

    #[test]
    fn profile_level_and_bias_stay_inside_their_clamps(
        rows in prop::collection::vec((0u32..1344u32, 0.0..5.0f64), 1..400)
    ) {
        let now = time("2026-03-06T00:00:00+00:00");
        let samples: Vec<LoadSample> = rows
            .iter()
            .map(|(offset, load_kw)| LoadSample {
                moment: now - Duration::hours(i64::from(*offset)),
                load_kw: *load_kw,
            })
            .collect();
        let profile = build_profile(&samples, &BTreeMap::new(), now, AMSTERDAM, 56, false);
        prop_assert!((0.5..=2.0).contains(&profile.scale), "scale {}", profile.scale);
        for bias in profile.bias {
            prop_assert!((0.5..=2.0).contains(&bias), "bias {bias}");
        }
    }

    #[test]
    fn decay_stays_inside_the_served_range(
        factors in prop::array::uniform24(any::<f64>()),
        hours in 0i64..2000
    ) {
        let now = time("2026-03-06T00:00:00+00:00");
        let updated = if hours == 0 { None } else { Some(now - Duration::hours(hours)) };
        let decayed = decay(&factors, updated, now);
        for factor in decayed {
            prop_assert!((0.3..=1.8).contains(&factor), "factor {factor}");
        }
    }

    #[test]
    fn a_single_source_merges_to_its_own_interpolation(
        values in prop::collection::vec(-5.0..5.0f64, 1..20),
        step in 1i64..5
    ) {
        let start = time("2026-03-06T06:00:00+00:00");
        let samples: Vec<_> = values
            .iter()
            .enumerate()
            .map(|(index, value)| (start + Duration::hours(index as i64 * step), *value))
            .collect();
        let source = SolarSource::new(
            "solo",
            SourceKind::Capacity,
            2.5,
            SourceState::Ok,
            Series::new(samples),
        );
        let grid: Vec<_> = (0..12).map(|index| start + Duration::minutes(90 * index)).collect();
        let outcome = merge_solar_series(&grid, std::slice::from_ref(&source), MergeStrategy::WeightedMean);
        for (index, slot) in grid.iter().enumerate() {
            let expected = source.series.interpolate(*slot).max(0.0);
            // A weighted mean of one value is `v * w / w`, which can differ from `v` by one ULP.
            prop_assert!((outcome.values[index] - expected).abs() <= 1e-12);
        }
    }
}

#[test]
fn night_always_returns_zero_without_perturbing_the_inputs() {
    let model = model();
    for hour in 0..4 {
        let at = time("2026-12-21T00:00:00+00:00") + Duration::hours(hour);
        let mut inputs = SlotInputs::new(
            at,
            Site::new(52.0, 5.0, 35.0, 180.0),
            4000.0,
            4000.0,
            600.0,
            12.0,
        );
        inputs.cloud_pct = 30.0;
        inputs.attenuation = 0.9;
        let estimate = estimate_slot(&model, &inputs).expect("night estimate");
        assert!(estimate.elevation_deg <= 0.0);
        assert_eq!(estimate.pv_kw, 0.0);
        assert_eq!(estimate.ratio, 0.0);
        assert_eq!(estimate.physics_w, 0.0);
        assert_eq!(estimate.poa_wm2, 0.0);
        assert_eq!(
            estimate.t_cell_c, 12.0,
            "the ambient temperature is passed through"
        );
    }
}

#[test]
fn capacity_output_is_monotone_in_cloud() {
    let at = time("2026-06-21T12:00:00+00:00");
    let config = CapacityConfig::new(4000.0);
    let site = Site::new(52.0, 5.0, 35.0, 180.0);
    let mut previous = f64::INFINITY;
    for step in 0..=100 {
        let cloud = f64::from(step);
        let slot =
            energy_forecast::solar::capacity_slot(&config, &site, at, AMSTERDAM, Some(cloud), None);
        assert!(slot.pv_kw <= previous + 1e-12, "cloud {cloud}");
        assert!(slot.pv_kw >= 0.0);
        previous = slot.pv_kw;
    }
}

#[test]
fn the_devices_never_double_count_a_missing_hour() {
    let start = time("2026-01-05T00:00:00+00:00");
    let house: Vec<LoadSample> = (0..120)
        .map(|hour| LoadSample {
            moment: start + Duration::hours(hour),
            load_kw: 2.0,
        })
        .collect();
    let device = LoadDevice::new(
        "sensor.ev",
        &energy_forecast::load::DeviceKind::parse("ev_charger"),
        "",
    );
    let series: BTreeMap<_, _> = (0..120)
        .filter(|hour| hour % 3 != 0)
        .map(|hour| (start + Duration::hours(hour), 0.5))
        .collect();
    let mut map = BTreeMap::new();
    map.insert(device.key.clone(), series);
    let split = split_device_load(&house, &[device], &map);
    assert_eq!(split.subtracted.len(), 1);
    assert_eq!(split.base.len(), 80);
    assert_eq!(split.dropped_hours, 40);
    for sample in &split.base {
        assert!((sample.load_kw - 1.5).abs() < 1e-12);
        assert_ne!(
            (sample.moment - start).num_hours() % 3,
            0,
            "missing device hour was kept"
        );
    }
}

#[test]
fn fallback_serving_uses_the_default_shape() {
    let now = time("2026-03-06T00:00:00+00:00");
    let profiles = BTreeMap::new();
    let temperatures = BTreeMap::new();
    let inputs = LoadInputs::new(&profiles, &temperatures, 24.0, now, AMSTERDAM, Some(12));
    let forecast = energy_forecast::load::load_forecast(None, &[], &inputs);
    assert_eq!(forecast.source, LoadSource::Fallback);
    // Over a full day the fallback shape integrates to exactly the configured base level.
    let total: f64 = forecast.slots.iter().map(|slot| slot.load_kw).sum::<f64>() / 4.0;
    assert_eq!(forecast.slots.len(), 48);
    let day_inputs = LoadInputs::new(&profiles, &temperatures, 24.0, now, AMSTERDAM, Some(24));
    let day = energy_forecast::load::load_forecast(None, &[], &day_inputs);
    let day_total: f64 = day.slots.iter().map(|slot| slot.load_kw).sum::<f64>() / 4.0;
    assert!(
        (day_total - 24.0).abs() < 0.5,
        "a day integrates to {day_total}, not 24"
    );
    assert!(total > 0.0);
}

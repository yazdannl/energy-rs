//! The frozen feature contract: names, order, digest and the vector's shape.

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
use energy_forecast::features::{FEATURE_COUNT, FEATURE_NAMES, Features, index, is_night};
use energy_forecast::model::feature_digest;
use energy_forecast::physics::Site;
use energy_forecast::pv::{SlotInputs, slot_features};
use support::model;

const CONTRACT: [&str; 28] = [
    "sun_elevation_deg",
    "cos_zenith",
    "cos_incidence",
    "sun_azimuth_sin",
    "sun_azimuth_cos",
    "tilt_deg",
    "tilt_sin",
    "ghi_wm2",
    "poa_wm2",
    "clearness_index",
    "diffuse_fraction",
    "dni_wm2",
    "t_ambient_c",
    "t_cell_c",
    "dc_capacity_kwp",
    "ac_capacity_kwp",
    "dc_ac_ratio",
    "clipping_headroom",
    "hour_sin",
    "hour_cos",
    "doy_sin",
    "doy_cos",
    "cloud_pct",
    "attenuation",
    "clear_sky_ghi_wm2",
    "wind_ms",
    "rh_pct",
    "rain_mm",
];

#[test]
fn feature_names_are_the_frozen_contract() {
    assert_eq!(FEATURE_COUNT, 28);
    assert_eq!(FEATURE_NAMES, CONTRACT);
    assert_eq!(feature_digest(), "028ba35d4f44d148");
    assert_eq!(index::ELEVATION_DEG, 0);
    assert_eq!(index::CLEARNESS_INDEX, 9);
    assert_eq!(index::T_CELL_C, 13);
    assert_eq!(index::CLOUD_PCT, 22);
    assert_eq!(index::RAIN_MM, 27);
    for column in 0..FEATURE_COUNT {
        assert_eq!(FEATURE_NAMES[column], CONTRACT[column]);
    }
}

#[test]
fn a_feature_vector_cannot_be_built_from_the_wrong_length() {
    let short = vec![0.0; 27];
    let long = vec![0.0; 29];
    assert!(matches!(
        Features::from_slice(&short),
        Err(ForecastError::FeatureContract(_))
    ));
    assert!(matches!(
        Features::from_slice(&long),
        Err(ForecastError::FeatureContract(_))
    ));
    assert!(Features::from_slice(&[0.0; 28]).is_ok());
}

#[test]
fn the_builder_produces_a_finite_vector_in_contract_order() {
    let moment = chrono::DateTime::parse_from_rfc3339("2026-06-21T12:00:00+00:00")
        .unwrap()
        .to_utc();
    let inputs = SlotInputs::new(
        moment,
        Site::new(52.0, 5.0, 35.0, 180.0),
        4000.0,
        4000.0,
        640.0,
        18.0,
    );
    let (built, physics_w, poa, elevation) = slot_features(&inputs).expect("daytime");
    assert!(elevation > 0.0);
    assert!(physics_w > 0.0);
    assert!(poa > 0.0 && poa < 1100.0);
    for (column, name) in FEATURE_NAMES.iter().enumerate() {
        assert!(
            built.get(column).is_finite(),
            "feature {name} is not finite"
        );
    }
    assert_eq!(built.get(index::TILT_DEG), 35.0);
    assert_eq!(built.get(index::DC_CAPACITY_KWP), 4.0);
    assert_eq!(built.get(index::AC_CAPACITY_KWP), 4.0);
    assert_eq!(built.get(index::DC_AC_RATIO), 1.0);
    assert_eq!(built.get(index::CLOUD_PCT), 0.0);
    assert_eq!(built.get(index::ATTENUATION), 1.0);
    // The UTC hour term is truncated to the hour, as training saw it.
    assert_eq!(
        built.get(index::HOUR_SIN).to_bits(),
        (2.0 * std::f64::consts::PI * 12.0 / 24.0).sin().to_bits()
    );
    assert_eq!(
        built.get(index::HOUR_COS).to_bits(),
        (2.0 * std::f64::consts::PI * 12.0 / 24.0).cos().to_bits()
    );
    // And the model consumes exactly this row.
    let ratio = model().ratio(&built).expect("walk");
    assert!((0.3..=1.8).contains(&ratio));
}

#[test]
fn the_night_gate_is_zero_degrees_not_five() {
    assert!(is_night(0.0));
    assert!(is_night(-0.001));
    assert!(!is_night(0.001), "elevation in (0, 5] is served, not gated");
    assert!(!is_night(4.9));
    assert_eq!(energy_forecast::features::MIN_ELEVATION_DEG, 5.0);
}

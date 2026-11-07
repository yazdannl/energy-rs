//! Physics unit tests: the two chains, their constants and the places they must not be unified.

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

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use energy_forecast::physics::{
    CapacityConfig, Site, ac_power, cell_temperature, clear_sky_ghi, cloud_attenuation,
    cloud_pct_from_attenuation, dc_power, erbs_diffuse_fraction, plane_irradiance, plane_of_array,
    solar_geometry, sun_terms,
};
use energy_forecast::solar::{capacity_poa, capacity_slot, uncertainty_pct};
use support::{read_json, series, time};

const AMSTERDAM: Tz = Tz::Europe__Amsterdam;

fn moment(text: &str) -> DateTime<Utc> {
    time(text)
}

#[test]
fn clear_sky_is_zero_at_and_below_the_horizon_and_below_1100() {
    assert_eq!(clear_sky_ghi(0.0), 0.0);
    assert_eq!(clear_sky_ghi(-0.5), 0.0);
    let zenith = clear_sky_ghi(1.0);
    assert!(zenith > 1000.0 && zenith < 1100.0, "{zenith}");
    // Haurwitz peaks around cos(zenith) ~ 0.94, never above the solar constant.
    for step in 1..=100 {
        let cos_zenith = f64::from(step) / 100.0;
        assert!(clear_sky_ghi(cos_zenith) < 1100.0);
    }
}

#[test]
fn erbs_branches_are_continuous() {
    let at_low = erbs_diffuse_fraction(0.22);
    let below_low = erbs_diffuse_fraction(0.219_999);
    assert!((at_low - below_low).abs() < 1e-6, "{at_low} {below_low}");
    // The shipped Erbs form steps by ~2.7e-4 at 0.80, because the polynomial does not reach 0.165
    // there. That step is served behaviour (Python has it too), so pin it instead of smoothing it.
    let at_high = erbs_diffuse_fraction(0.80);
    let above_high = erbs_diffuse_fraction(0.800_001);
    assert_eq!(above_high, 0.165);
    assert!((at_high - 0.165).abs() < 3e-4, "{at_high}");
    assert!(at_high > above_high);
    assert_eq!(erbs_diffuse_fraction(0.0), 1.0);
    assert_eq!(erbs_diffuse_fraction(1.0), 0.165);
    // Monotone decrease through the polynomial branch.
    let mut previous = erbs_diffuse_fraction(0.23);
    for step in 24..80 {
        let current = erbs_diffuse_fraction(f64::from(step) / 100.0);
        assert!(current <= previous + 1e-12);
        previous = current;
    }
}

#[test]
fn hour_angle_is_antisymmetric_about_solar_noon() {
    let latitude = 52.0;
    let longitude = 5.0;
    let day = moment("2026-06-21T00:00:00+00:00");
    let site = Site::new(latitude, longitude, 35.0, 180.0);
    let noon = solar_geometry(&site, day);
    let _ = noon;
    // Solar noon is 12:00 minus EOT minus the longitude offset, so compare equal offsets from it.
    let declination = energy_forecast::physics::declination_rad(172);
    let eot = energy_forecast::physics::equation_of_time_min(172);
    let noon_hour = (720.0 - eot - 4.0 * longitude) / 60.0;
    let before = energy_forecast::physics::hour_angle_rad(longitude, 172, noon_hour - 3.0);
    let after = energy_forecast::physics::hour_angle_rad(longitude, 172, noon_hour + 3.0);
    let _ = sun_terms(longitude, day, AMSTERDAM);
    assert!((before + after).abs() < 1e-12, "{before} {after}");
    let latitude_rad = latitude.to_radians();
    let elevation_before =
        energy_forecast::physics::elevation_deg(latitude_rad, declination, before);
    let elevation_after = energy_forecast::physics::elevation_deg(latitude_rad, declination, after);
    assert!((elevation_before - elevation_after).abs() < 1e-9);
}

#[test]
fn the_two_poa_chains_stay_apart() {
    // The spec's measured case: 52N, 5E, 2026-12-23 08:45Z.
    let at = moment("2026-12-23T08:45:00+00:00");
    let geometry = solar_geometry(&Site::new(52.0, 5.0, 35.0, 180.0), at);
    let learned = plane_of_array(
        geometry.clear_sky_ghi_wm2,
        geometry.cos_zenith,
        geometry.cos_incidence,
        35.0,
    );
    let capacity = plane_irradiance(&Site::new(52.0, 5.0, 35.0, 180.0), at, AMSTERDAM);
    assert!(
        (capacity - learned.poa).abs() > 100.0,
        "the two chains must not be unified: learned {} capacity {capacity}",
        learned.poa
    );
    assert!((learned.poa - 110.16740417815824).abs() < 1e-9);
    assert!((capacity - 247.33757676175694).abs() < 1e-9);
}

#[test]
fn power_chain_clips_and_derates() {
    assert_eq!(cell_temperature(20.0, 800.0), 20.0 + 0.03125 * 800.0);
    let dc = dc_power(4000.0, 1000.0, 25.0);
    assert!((dc - 4000.0).abs() < 1e-9);
    // Cold cells produce more DC than at STC.
    assert!(dc_power(4000.0, 1000.0, 0.0) > 4000.0);
    // AC clips at the inverter limit.
    assert_eq!(ac_power(4000.0, 3000.0, 1000.0, 25.0), 3000.0);
    assert_eq!(ac_power(4000.0, 2600.0, 1000.0, 25.0), 2600.0);
    let unclipped = ac_power(4000.0, 5000.0, 1000.0, 25.0);
    assert!((unclipped - 3840.0).abs() < 1e-9);
    // Negative DC is floored before the inverter.
    assert_eq!(ac_power(4000.0, 5000.0, 0.0, 60.0), 0.0);
}

#[test]
fn cloud_attenuation_round_trips_and_is_monotone() {
    let mut previous = cloud_attenuation(0.0);
    assert_eq!(previous, 1.0);
    for step in 1..=100 {
        let cloud = f64::from(step);
        let current = cloud_attenuation(cloud);
        assert!(current <= previous + 1e-15);
        assert!((0.25..=1.0).contains(&current));
        previous = current;
    }
    assert_eq!(cloud_attenuation(100.0), 0.25);
    assert_eq!(cloud_attenuation(f64::NAN), 1.0);
    for cloud in [0.0, 10.0, 37.5, 60.0, 100.0] {
        let attenuation = cloud_attenuation(cloud);
        let recovered = cloud_pct_from_attenuation(attenuation);
        assert!((recovered - cloud).abs() < 1e-9, "{cloud} -> {recovered}");
    }
}

#[test]
fn geometry_puts_the_sun_where_it_belongs() {
    let summer_noon = moment("2026-06-21T12:00:00+00:00");
    let winter_midnight = moment("2026-12-21T00:00:00+00:00");
    let site = Site::new(52.0, 5.0, 35.0, 180.0);
    let noon = solar_geometry(&site, summer_noon);
    let night = solar_geometry(&site, winter_midnight);
    assert!(noon.elevation_deg > 60.0 && noon.elevation_deg < 63.0);
    assert!(night.elevation_deg < 0.0);
    assert!(noon.clear_sky_ghi_wm2 > 850.0 && noon.clear_sky_ghi_wm2 < 950.0);
    // The solar azimuth is measured from north, clockwise; at noon in Utrecht it is southerly.
    assert!((noon.sun_azimuth_deg - 180.0).abs() < 15.0);
    // cos(zenith) == cos(radians(90 - elevation)) by construction.
    let expected = (90.0_f64 - noon.elevation_deg).to_radians().cos();
    assert_eq!(noon.cos_zenith.to_bits(), expected.to_bits());
}

#[test]
fn capacity_slots_match_the_reference() {
    let fixture = read_json(&support::fixture("solar_pure.json"));
    let slots = fixture["capacity_slots"]
        .as_array()
        .expect("capacity slots");
    assert_eq!(slots.len(), 30);
    for (index, slot) in slots.iter().enumerate() {
        let moment = DateTime::from_timestamp(slot["epoch"].as_i64().expect("epoch"), 0)
            .expect("epoch is valid");
        let config = CapacityConfig {
            capacity_wp: slot["capacity_wp"].as_f64().unwrap(),
            performance_ratio: slot["performance_ratio"].as_f64().unwrap(),
        };
        let site = Site::new(
            slot["lat"].as_f64().unwrap(),
            slot["lon"].as_f64().unwrap(),
            slot["tilt_deg"].as_f64().unwrap(),
            slot["azimuth_deg"].as_f64().unwrap(),
        );
        let cloud = slot["cloud_pct"].as_f64();
        let nasa = slot["nasa_clearness"].as_f64();
        let result = capacity_slot(&config, &site, moment, AMSTERDAM, cloud, nasa);
        assert_eq!(
            result.pv_kw.to_bits(),
            slot["pv_kw"].as_f64().unwrap().to_bits(),
            "slot {index} pv_kw"
        );
        assert_eq!(
            result.attenuation.to_bits(),
            slot["attenuation"].as_f64().unwrap().to_bits(),
            "slot {index} attenuation"
        );
        assert_eq!(
            result.uncertainty_pct,
            slot["uncertainty_pct"].as_f64().unwrap(),
            "slot {index} uncertainty"
        );
        let poa = capacity_poa(&site, moment, AMSTERDAM);
        assert_eq!(
            poa.to_bits(),
            slot["poa_wm2"].as_f64().unwrap().to_bits(),
            "slot {index} poa"
        );
    }
    assert_eq!(uncertainty_pct(true, false), 25.0);
    assert_eq!(uncertainty_pct(false, true), 30.0);
    assert_eq!(uncertainty_pct(false, false), 35.0);
}

#[test]
fn pure_series_helper_is_used_for_the_derived_fit_fixture() {
    let fixture = read_json(&support::fixture("solar_pure.json"));
    let derived = &fixture["derived"];
    let samples = series(&derived["samples"]);
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
    let fitted =
        energy_forecast::solar::implied_capacity_wp(&site, &samples, &config, None, AMSTERDAM);
    assert_eq!(fitted.samples, derived["usable"].as_u64().unwrap() as usize);
    assert_eq!(
        fitted.capacity_wp.to_bits(),
        derived["capacity_wp"].as_f64().unwrap().to_bits()
    );
}

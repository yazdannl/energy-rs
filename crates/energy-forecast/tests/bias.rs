//! The correction layer: one factor, applied once, head replacing EWMA.

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

use chrono::Duration;
use chrono_tz::Tz;
use energy_forecast::bias::{
    Correction, HeadProfile, HeadSample, HeadStatus, HourlyBias, evaluate, fit_factors,
};
use support::time;

const AMSTERDAM: Tz = Tz::Europe__Amsterdam;

fn samples(count: usize, forecast: f64, actual: f64) -> Vec<HeadSample> {
    (0..count)
        .map(|index| HeadSample {
            moment: time("2026-03-01T00:00:00+00:00")
                + Duration::hours(i64::try_from(index).unwrap_or(0)),
            hour: u32::try_from(index).unwrap_or(0) % 24,
            forecast_kw: forecast + index as f64 * 0.05,
            actual_kw: actual + index as f64 * 0.05,
        })
        .collect()
}

fn head(promoted: bool, factor: f64) -> HeadProfile {
    let mut factors = [1.0_f64; 24];
    factors.fill(factor);
    HeadProfile {
        status: if promoted { "promoted" } else { "rejected" }.to_string(),
        promoted,
        factors,
        fitted_at: Some(time("2026-03-01T00:00:00+00:00")),
        kind: "solar".to_string(),
        samples: 100,
        fit_samples: 80,
        holdout_samples: 20,
        candidate_mae_kw: Some(0.2),
        incumbent_mae_kw: Some(0.3),
        shrinkage: 25.0,
        reason: "test".to_string(),
        previous: None,
        reverted_at: None,
    }
}

#[test]
fn a_promoted_head_replaces_the_ewma_instead_of_multiplying_with_it() {
    let now = time("2026-03-07T12:00:00+00:00");
    let mut bias = HourlyBias::neutral();
    bias.factors.fill(1.5);
    bias.updated_at = Some(now);
    let with_head = Correction::from_profiles(Some(head(true, 0.8)), bias.clone());
    assert_eq!(with_head.kind(), "head");
    assert!((with_head.factor(12, now) - 0.8).abs() < 1e-12, "head only");
    let without_head = Correction::from_profiles(Some(head(false, 0.8)), bias.clone());
    assert_eq!(without_head.kind(), "ewma");
    assert!(
        (without_head.factor(12, now) - 1.5).abs() < 1e-12,
        "ewma only"
    );
    let neutral = Correction::neutral();
    assert_eq!(neutral.kind(), "none");
    assert_eq!(neutral.factor(12, now), 1.0);
    assert!(neutral.is_neutral());
}

#[test]
fn the_factor_is_applied_once_and_floored_at_zero() {
    let now = time("2026-03-07T12:00:00+00:00");
    let correction = Correction::from_profiles(Some(head(true, 1.25)), HourlyBias::neutral());
    assert!((correction.apply(2.0, 3, now) - 2.5).abs() < 1e-12);
    assert_eq!(correction.apply(-4.0, 3, now), 0.0);
    // Applying the correction twice is distinguishable from applying it once; the caller must not.
    let once = correction.apply(2.0, 3, now);
    let twice = correction.apply(once, 3, now);
    assert!((twice - 3.125).abs() < 1e-12);
}

#[test]
fn ewma_hours_are_independent_and_clamped() {
    let now = time("2026-03-07T12:00:00+00:00");
    let mut bias = HourlyBias::neutral();
    bias.factors[5] = 9.0;
    bias.factors[9] = 0.01;
    let factors = bias.factors_at(now);
    assert_eq!(factors[5], 1.8);
    assert_eq!(factors[9], 0.3);
    assert_eq!(factors[6], 1.0);
}

#[test]
fn ewma_update_needs_a_real_forecast_and_a_non_negative_measurement() {
    let now = time("2026-03-07T12:00:00+00:00");
    let mut bias = HourlyBias::neutral();
    bias.update(
        &[
            energy_forecast::bias::BiasSample {
                moment: time("2026-03-07T10:00:00+00:00"),
                forecast_kw: 0.01,
                measured_kw: 1.0,
            },
            energy_forecast::bias::BiasSample {
                moment: time("2026-03-07T10:00:00+00:00"),
                forecast_kw: 1.0,
                measured_kw: -0.5,
            },
        ],
        now,
        AMSTERDAM,
    );
    assert_eq!(bias.counts, [0; 24]);
    assert_eq!(bias.mape_pct, None);
    assert_eq!(bias.factors, [1.0; 24]);
}

#[test]
fn the_head_gate_never_promotes_on_noise() {
    let fit = samples(40, 2.0, 2.0);
    // Equal, non-zero errors on the holdout: the candidate cannot beat the incumbent by 2%.
    let holdout: Vec<HeadSample> = samples(10, 2.0, 2.1);
    let incumbent = fit_factors(&fit, 25.0);
    // The candidate is the incumbent exactly: nothing can beat it by 2%, so nothing is promoted.
    let report = evaluate(&fit, &holdout, &incumbent);
    assert_eq!(report.status, HeadStatus::Rejected);
    assert!(!report.promoted);
    assert!(report.reason.contains("keeping the incumbent"));
    assert!(report.incumbent_mae_kw.unwrap() > 0.0);
    // An exact incumbent is refused outright, whatever the candidate does.
    let exact: Vec<HeadSample> = samples(10, 2.0, 2.0);
    let report = evaluate(&fit, &exact, &[1.0; 24]);
    assert_eq!(report.status, HeadStatus::Rejected);
    assert!(report.reason.contains("already exact"));
}

#[test]
fn an_insufficient_head_leaves_the_ewma_in_force() {
    let fit = samples(10, 1.0, 0.8);
    let holdout = samples(5, 1.0, 0.8);
    let report = evaluate(&fit, &holdout, &[1.0; 24]);
    assert_eq!(report.status.as_str(), "insufficient");
    assert!(report.candidate_mae_kw.is_none());
}

#[test]
fn the_head_fit_is_shrunk_toward_one_and_clamped() {
    assert_eq!(fit_factors(&[], 25.0), [1.0; 24]);
    let strong: Vec<HeadSample> = (0..200)
        .map(|index| HeadSample {
            moment: time("2026-03-01T00:00:00+00:00") + Duration::hours(index),
            hour: 12,
            forecast_kw: 2.0,
            actual_kw: 5.0,
        })
        .collect();
    let factors = fit_factors(&strong, 25.0);
    assert_eq!(factors[12], 1.8, "clamped at the served maximum");
    assert_eq!(factors[11], 1.0, "an unfitted hour stays neutral");
}

#[test]
fn a_rejected_head_can_be_reverted_into_the_previous_profile() {
    let previous = head(false, 1.1);
    let promoted = HeadProfile::from_report(
        &energy_forecast::bias::HeadReport {
            status: HeadStatus::Promoted,
            promoted: true,
            samples: 100,
            fit_samples: 80,
            holdout_samples: 20,
            candidate_mae_kw: Some(0.2),
            incumbent_mae_kw: Some(0.3),
            reason: "beat".to_string(),
        },
        [1.2; 24],
        "solar",
        time("2026-03-02T00:00:00+00:00"),
        Some(previous),
    );
    let mut restored = promoted.clone();
    assert!(restored.revert(time("2026-03-03T00:00:00+00:00")));
    assert_eq!(restored.status, "reverted");
    assert!(!restored.promoted);
    assert_eq!(restored.factors, [1.1; 24]);
    assert!(restored.reverted_at.is_some());
    // Nothing left to revert.
    assert!(!restored.revert(time("2026-03-04T00:00:00+00:00")));
}

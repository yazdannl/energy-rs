#![allow(clippy::unchecked_time_subtraction)]

use super::*;
use energy_core::stream::{EngineSnapshot, PackSnapshot};

fn snapshot_configured(
    tick: Option<i64>,
    samples: &[Option<i64>],
    configured_packs: usize,
) -> EngineSnapshot {
    let mut value = EngineSnapshot {
        last_tick: tick,
        configured_packs,
        ..EngineSnapshot::default()
    };
    for sampled_at in samples {
        value.packs.push(PackSnapshot {
            sampled_at: *sampled_at,
            ..PackSnapshot::default()
        });
    }
    value
}

fn snapshot(tick: Option<i64>, sample: Option<i64>) -> EngineSnapshot {
    snapshot_configured(tick, &[sample], 1)
}

#[allow(clippy::unchecked_time_subtraction)]
fn verdict(now: i64, tick: Option<i64>, sample: Option<i64>) -> (StatusCode, Health) {
    evaluate(
        &snapshot(tick, sample),
        now,
        Duration::from_secs(120),
        now - 200,
        Instant::now() - Duration::from_secs(200),
    )
}

#[test]
fn a_missing_tick_degrades_after_the_grace() {
    let (status, body) = evaluate(
        &snapshot_configured(None, &[Some(1_000)], 1),
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.reason, Some("tick_stalled"));
    assert_eq!(body.age_seconds, Some(200));
}

#[test]
fn missing_samples_degrade_when_packs_are_configured() {
    let (status, body) = evaluate(
        &snapshot_configured(Some(1_000), &[None], 1),
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.reason, Some("sampler_stalled"));
}

#[test]
fn both_missing_degrade_as_a_tick_stall() {
    let (status, body) = evaluate(
        &snapshot_configured(None, &[None], 1),
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.reason, Some("tick_stalled"));
}

#[test]
fn a_configured_fleet_publishing_no_packs_degrades() {
    let (status, body) = evaluate(
        &snapshot_configured(Some(1_000), &[], 4),
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.reason, Some("sampler_stalled"));
}

#[test]
fn a_configured_fleet_with_stale_samples_degrades() {
    let (status, body) = evaluate(
        &snapshot_configured(Some(1_000), &[Some(700)], 1),
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.reason, Some("sampler_stalled"));
}

#[test]
fn an_unconfigured_fleet_is_not_a_sampler_stall() {
    let (status, _body) = evaluate(
        &snapshot_configured(Some(1_000), &[], 0),
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::OK);
}

#[test]
fn an_uncertain_write_is_surfaced_in_the_health_reason() {
    let mut value = snapshot(Some(1_000), Some(1_000));
    value.packs[0].last_control_fault =
        Some("uncertain_write: device x request 1 timed out".to_owned());
    let (status, body) = evaluate(
        &value,
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.reason, Some("uncertain_write"));
}

#[test]
fn fresh_tick_and_sample_are_ok() {
    let (status, body) = verdict(1_000, Some(990), Some(995));
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.status, "ok");
    assert_eq!(body.reason, None);
}

#[test]
fn a_stale_tick_degrades_with_age_and_since() {
    let (status, body) = verdict(1_000, Some(879), Some(995));
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.reason, Some("tick_stalled"));
    assert_eq!(body.age_seconds, Some(121));
    assert!(body.tick_stalled_since.is_some());
}

#[test]
fn startup_grace_suppresses_the_check() {
    let (status, body) = evaluate(
        &snapshot(Some(0), Some(0)),
        10,
        Duration::from_secs(120),
        0,
        Instant::now(),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.status, "ok");
}

#[test]
fn a_stale_sample_degrades_even_with_a_fresh_tick() {
    let (status, body) = verdict(1_000, Some(999), Some(879));
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.reason, Some("sampler_stalled"));
    assert_eq!(body.age_seconds, Some(121));
    assert!(body.sampler_stalled_since.is_some());
}

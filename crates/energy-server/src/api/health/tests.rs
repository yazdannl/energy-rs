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
        required_packs: (0..samples.len()).map(|i| format!("SN-{i}")).collect(),
        ..EngineSnapshot::default()
    };
    for (index, sampled_at) in samples.iter().enumerate() {
        value.packs.push(PackSnapshot {
            sn: format!("SN-{index}"),
            sampled_at: *sampled_at,
            ..PackSnapshot::default()
        });
    }
    value
}

fn snapshot(tick: Option<i64>, sample: Option<i64>) -> EngineSnapshot {
    snapshot_configured(tick, &[sample], 1)
}

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
    let value = snapshot_configured(None, &[Some(1_000)], 1);
    let (status, body) = evaluate(
        &value,
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
fn a_missing_pack_sample_is_diagnostics_not_a_restart() {
    let value = snapshot_configured(
        Some(1_000),
        &[Some(1_000), Some(1_000), Some(1_000), None],
        4,
    );
    let (status, body) = evaluate(
        &value,
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.status, "ok");
    assert!(body.degraded);
    assert_eq!(body.stalled_packs.len(), 1);
    assert_eq!(body.stalled_packs[0].sn, "SN-3");
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
fn a_blind_fleet_publishing_no_packs_degrades() {
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
fn an_active_uncertain_write_is_surfaced_per_pack() {
    let mut value = snapshot_configured(Some(1_000), &[Some(1_000), Some(1_000)], 2);
    value.packs[1].active_control_fault =
        Some("uncertain_write: device B request 1 timed out".to_owned());
    let (status, body) = evaluate(
        &value,
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.reason, Some("uncertain_write"));
    assert_eq!(body.uncertain_packs, vec!["SN-1".to_owned()]);
}
#[test]
fn acknowledging_history_does_not_hide_an_unresolved_active_fault() {
    let mut value = snapshot_configured(Some(1_000), &[Some(1_000)], 1);
    value.packs[0].active_control_fault =
        Some("uncertain_write: device A request 1 timed out".to_owned());
    value.packs[0].last_control_fault = None;
    let (status, body) = evaluate(
        &value,
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.reason, Some("uncertain_write"));
    assert_eq!(body.uncertain_packs, vec!["SN-0".to_owned()]);
}
#[test]
fn a_historical_uncertain_write_does_not_degrade_health_forever() {
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
    assert_eq!(body.reason, None);
    assert_eq!(body.uncertain_packs, Vec::<String>::new());
}
#[test]
fn a_verified_recovery_clears_the_health_reason() {
    let mut value = snapshot(Some(1_000), Some(1_000));
    value.packs[0].active_control_fault =
        Some("uncertain_write: device x request 1 timed out".to_owned());
    value.packs[0].last_control_fault = value.packs[0].active_control_fault.clone();
    let args = |value: &EngineSnapshot| {
        evaluate(
            value,
            1_000,
            Duration::from_secs(120),
            800,
            Instant::now() - Duration::from_secs(200),
        )
    };
    assert_eq!(args(&value).1.reason, Some("uncertain_write"));
    value.packs[0].active_control_fault = None;
    let (status, body) = args(&value);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.reason, None);
    assert_eq!(body.uncertain_packs, Vec::<String>::new());
    assert!(value.packs[0].last_control_fault.is_some(), "history stays");
}
#[test]
fn fresh_tick_and_sample_are_ok() {
    let (status, body) = verdict(1_000, Some(990), Some(995));
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.status, "ok");
    assert_eq!(body.reason, None);
    assert!(!body.degraded);
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
    let (status, _body) = evaluate(
        &snapshot(Some(0), Some(0)),
        10,
        Duration::from_secs(120),
        0,
        Instant::now(),
    );
    assert_eq!(status, StatusCode::OK);
}
#[test]
fn three_fresh_and_one_stalled_stay_ok_but_name_the_pack() {
    let mut value = snapshot_configured(
        Some(1_000),
        &[Some(1_000), Some(1_000), Some(1_000), None],
        4,
    );
    for (index, sn) in ["SN-A", "SN-B", "SN-C", "SN-D"].iter().enumerate() {
        value.packs[index].sn = (*sn).to_owned();
    }
    value.required_packs = ["SN-A", "SN-B", "SN-C", "SN-D"]
        .iter()
        .map(|sn| (*sn).to_owned())
        .collect();
    let (status, body) = evaluate(
        &value,
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.status, "ok");
    assert!(body.degraded);
    assert_eq!(body.stalled_packs.len(), 1);
    assert_eq!(body.stalled_packs[0].sn, "SN-D");
    assert_eq!(body.stalled_packs[0].sample_age_seconds, 200);
}
#[test]
fn a_disabled_pack_is_never_a_required_stall() {
    let mut value = snapshot_configured(
        Some(1_000),
        &[Some(1_000), Some(1_000), Some(1_000), None],
        3,
    );
    for (index, sn) in ["SN-A", "SN-B", "SN-C", "SN-D"].iter().enumerate() {
        value.packs[index].sn = (*sn).to_owned();
    }
    value.required_packs = ["SN-A", "SN-B", "SN-C"]
        .iter()
        .map(|sn| (*sn).to_owned())
        .collect();
    let (status, body) = evaluate(
        &value,
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::OK);
    assert!(!body.degraded);
    assert_eq!(body.stalled_packs, Vec::<StalledPack>::new());
}
#[test]
fn four_stale_packs_are_fleet_blind_and_restart_worthy() {
    let value = snapshot_configured(
        Some(1_000),
        &[Some(700), Some(700), Some(700), Some(700)],
        4,
    );
    let (status, body) = evaluate(
        &value,
        1_000,
        Duration::from_secs(120),
        800,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.reason, Some("sampler_stalled"));
    assert_eq!(body.stalled_packs.len(), 4);
    assert_eq!(body.sampler_stalled_since, Timestamp::lenient(Some(700)));
}
#[test]
fn a_stalled_sample_is_since_its_own_reading_not_boot() {
    let mut value = snapshot(Some(1_000), Some(700));
    value.required_packs = vec!["SN-0".to_owned()];
    let (status, body) = evaluate(
        &value,
        1_000,
        Duration::from_secs(120),
        900,
        Instant::now() - Duration::from_secs(200),
    );
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "a one-pack fleet is blind"
    );
    assert_eq!(body.stalled_packs[0].sample_age_seconds, 300);
    assert_eq!(
        body.stalled_packs[0].last_seen_at,
        Timestamp::lenient(Some(700))
    );
}

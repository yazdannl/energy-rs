// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use energy_battery::{
    Command, ControlLimits, DeviceController, Endpoint, FakeBehaviour, FakeDevice, FakePack,
    FakeState, FleetClient, HttpTransport,
};
use tokio::time::Instant;

const SN_A: &str = "EOC1NLN9N4AAAAAA";
const SN_B: &str = "EOC1NLN9N4BBBBBB";

fn fast_behaviour() -> FakeBehaviour {
    FakeBehaviour::default().without_spacing_rule()
}

async fn fake(sn: &str, state: FakeState) -> FakeDevice {
    FakeDevice::spawn(state.with_behaviour(fast_behaviour()))
        .await
        .unwrap_or_else(|error| panic!("fake {sn}: {error}"))
}

fn controller(device: &FakeDevice, sn: &str) -> Arc<DeviceController<Arc<HttpTransport>>> {
    let transport =
        HttpTransport::with_timeout(device.endpoint(sn), Duration::from_secs(2)).unwrap();
    Arc::new(DeviceController::new(
        Arc::new(transport),
        ControlLimits::unlimited(),
    ))
}

#[tokio::test]
async fn aggregates_soc_by_capacity_and_sums_power() {
    let first = fake(SN_A, FakeState::new(SN_A).with_soc(45.0)).await;
    let mut second_state = FakeState::new(SN_B).with_soc(60.0).with_packs(vec![
        FakePack::ab2000("CO4A000000000002"),
        FakePack::ab2000("CO4A000000000003"),
    ]);
    second_state.set_power(200, 0);
    let second = fake(SN_B, second_state).await;
    let mut first_state = first.state().unwrap();
    first_state.set_power(0, 300);
    first.with_state(|state| *state = first_state).unwrap();

    let fleet = FleetClient::with_interval(
        vec![controller(&first, SN_A), controller(&second, SN_B)],
        Duration::from_secs(5),
    );
    let snapshot = fleet.refresh().await;
    assert_eq!(snapshot.aggregate.online, 2);
    assert_eq!(snapshot.aggregate.total, 2);
    assert_eq!(snapshot.aggregate.capacity_kwh, Some(1.92 + 3.84));
    assert_eq!(snapshot.aggregate.power_w, 100);
    let expected = (45.0 * 1.92 + 60.0 * 3.84) / (1.92 + 3.84);
    assert!((snapshot.aggregate.soc_weighted_pct.unwrap() - expected).abs() < 1e-9);
    assert!(snapshot.is_complete());
}

#[tokio::test(start_paused = true)]
async fn polling_is_staggered_by_the_interval() {
    let first = fake(SN_A, FakeState::new(SN_A)).await;
    let second = fake(SN_B, FakeState::new(SN_B)).await;
    let fleet = FleetClient::with_interval(
        vec![controller(&first, SN_A), controller(&second, SN_B)],
        Duration::from_secs(5),
    );
    assert_eq!(fleet.offset(), Duration::from_millis(2500));

    let snapshot = fleet.poll().await;
    assert_eq!(snapshot.aggregate.online, 1, "only the first slot is due");

    tokio::time::advance(Duration::from_millis(2500)).await;
    let snapshot = fleet.poll().await;
    assert_eq!(snapshot.aggregate.online, 2);
    assert!(snapshot.is_complete());

    // Nothing is due again until the interval has passed.
    let before = snapshot.fetched_at;
    let snapshot = fleet.poll().await;
    assert_eq!(snapshot.fetched_at, before);
}

#[tokio::test]
async fn a_dead_device_is_a_partial_failure_not_an_abort() {
    let alive = fake(SN_A, FakeState::new(SN_A)).await;
    let dead_endpoint = Endpoint::with_port("127.0.0.1", 1, SN_B);
    let dead = Arc::new(DeviceController::new(
        Arc::new(HttpTransport::with_timeout(dead_endpoint, Duration::from_millis(200)).unwrap()),
        ControlLimits::unlimited(),
    ));
    let fleet = FleetClient::with_interval(vec![controller(&alive, SN_A), dead], Duration::ZERO);
    let snapshot = fleet.refresh().await;
    assert_eq!(snapshot.aggregate.online, 1);
    assert!(!snapshot.is_complete());
    let failures: Vec<_> = snapshot.failures().collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].sn, SN_B);
    assert!(failures[0].error.is_some());
    assert_eq!(snapshot.device(SN_A).unwrap().soc_pct(), Some(45.0));
}

#[tokio::test]
async fn fanout_reports_every_device_and_a_partial_failure() {
    let first = fake(SN_A, FakeState::new(SN_A)).await;
    let second = fake(SN_B, FakeState::new(SN_B)).await;
    let fleet = FleetClient::with_interval(
        vec![controller(&first, SN_A), controller(&second, SN_B)],
        Duration::ZERO,
    );
    fleet.refresh().await;

    second
        .with_state(|state| state.behaviour = FakeBehaviour::default().wedged())
        .unwrap();
    fleet.refresh().await;

    let report = fleet.fanout(|_| Some(Command::Charge { watts: 300 })).await;
    assert_eq!(report.accepted_count(), 1);
    assert_eq!(report.failed_count(), 1);
    assert!(!report.is_complete());
    let failure = report.failed().next().unwrap();
    assert_eq!(failure.sn, SN_B);
    assert!(failure.error().unwrap().to_string().contains("wedged"));
    assert_eq!(report.skipped, 0);
}

#[tokio::test]
async fn fanout_can_skip_devices() {
    let first = fake(SN_A, FakeState::new(SN_A)).await;
    let second = fake(SN_B, FakeState::new(SN_B)).await;
    let fleet = FleetClient::with_interval(
        vec![controller(&first, SN_A), controller(&second, SN_B)],
        Duration::ZERO,
    );
    fleet.refresh().await;
    let report = fleet
        .fanout(|device| (device.sn == SN_A).then_some(Command::Discharge { watts: 200 }))
        .await;
    assert_eq!(report.accepted_count(), 1);
    assert_eq!(report.skipped, 1);
    assert!(!report.is_complete());
    assert_eq!(report.outcomes[0].sn, SN_A);
    assert_eq!(
        report.outcomes[0].command,
        Command::Discharge { watts: 200 }
    );
    let applied = second.state().unwrap();
    assert_eq!(applied.write_count(), 0);
}

#[tokio::test(start_paused = true)]
async fn freshness_is_measured_against_the_last_read() {
    let first = fake(SN_A, FakeState::new(SN_A)).await;
    let fleet = FleetClient::with_interval(vec![controller(&first, SN_A)], Duration::from_secs(30));
    let snapshot = fleet.refresh().await;
    let device = snapshot.device(SN_A).unwrap();
    assert!(device.is_fresh(Instant::now(), Duration::from_secs(30)));
    tokio::time::advance(Duration::from_secs(31)).await;
    assert!(!device.is_fresh(Instant::now(), Duration::from_secs(30)));
}

//! The engine/server contract: a snapshot per tick, and API commands applied between ticks.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use std::sync::Arc;

use energy_core::dispatch::OverrideAction;
use energy_core::engine::{Engine, EngineConfig};
use energy_core::safety::Authority;
use energy_core::stream::{
    EngineCommand, EngineSnapshot, FaultSnapshot, OverrideCommand, RuntimeView, SiteSnapshot,
    SiteSource, SnapshotPublisher, WatchPublisher,
};
use support::fakes::{self, NOW, pack, plan};

/// A discovery view with one source and one server-side fault.
struct View;

impl RuntimeView for View {
    fn site(&self) -> SiteSnapshot {
        SiteSnapshot {
            configured: true,
            sources: vec![SiteSource {
                kind: "grid".to_owned(),
                entity_id: "sensor.grid_power".to_owned(),
                label: None,
            }],
            capabilities: vec!["battery_control".to_owned()],
        }
    }

    fn faults(&self) -> Vec<FaultSnapshot> {
        vec![FaultSnapshot {
            kind: "prices_stale".to_owned(),
            message: "the newest publication is six hours old".to_owned(),
            since: NOW - 21_600,
        }]
    }
}

fn engine_with_stream(mode: Authority, packs: usize) -> (Arc<Engine>, Arc<WatchPublisher>) {
    let readings: Vec<_> = (0..packs)
        .map(|index| pack(&format!("SN-{index}"), 50.0))
        .collect();
    let ids: Vec<&str> = readings
        .iter()
        .map(energy_core::dispatch::PackReading::pack_id)
        .collect();
    let ledger = fakes::ledger_with(&ids);
    let engine = Engine::new(
        EngineConfig {
            mode,
            ..EngineConfig::default()
        },
        Arc::new(fakes::FakeFleet::new(readings)),
        Arc::new(fakes::FakeMeter::default()),
        Arc::new(fakes::FakeSource::new(plan(NOW, -0.8, 0.96))),
        Arc::new(fakes::FakeSink::new()),
        Arc::clone(&ledger),
    );
    let publisher = Arc::new(WatchPublisher::empty());
    let engine = Arc::new(engine.with_stream(
        Arc::clone(&publisher) as Arc<dyn SnapshotPublisher>,
        Arc::new(View),
    ));
    (engine, publisher)
}

#[tokio::test]
async fn a_tick_publishes_the_read_model() {
    let (engine, publisher) = engine_with_stream(Authority::DryRun, 2);
    engine.boot(NOW).await.expect("boot");
    assert!(publisher.current().last_tick.is_none());
    engine.tick_once(NOW).await.expect("tick");

    let snapshot = publisher.current();
    assert_eq!(snapshot.mode, Authority::DryRun);
    assert_eq!(snapshot.last_tick, Some(NOW));
    assert_eq!(snapshot.packs.len(), 2);
    assert!(snapshot.packs.iter().all(|pack| pack.soc_pct.is_some()));
    assert!(snapshot.site.configured);
    assert!(snapshot.plan.as_ref().is_some_and(|plan| plan.plan_id > 0));
    assert!(snapshot.forecast.is_some());
    assert!(
        snapshot
            .faults
            .iter()
            .any(|fault| fault.kind == "prices_stale"),
        "the server fault is missing: {:?}",
        snapshot.faults
    );
    assert!(
        snapshot
            .faults
            .iter()
            .any(|fault| fault.kind == "meter_missing"),
        "an empty meter must be a fault"
    );
}

#[tokio::test]
async fn a_command_takes_effect_and_is_published() {
    let (engine, publisher) = engine_with_stream(Authority::DryRun, 1);
    engine.boot(NOW).await.expect("boot");
    engine.tick_once(NOW).await.expect("tick");

    engine
        .apply_command(EngineCommand::SetMode(Authority::Auto), NOW + 1)
        .await
        .expect("set_mode");
    assert_eq!(engine.mode().expect("mode"), Authority::Auto);
    assert_eq!(publisher.current().mode, Authority::Auto);

    engine
        .apply_command(
            EngineCommand::Override(OverrideCommand {
                pack: "SN-0".to_owned(),
                action: OverrideAction::Charge { watts: 200.0 },
                ttl_s: 600,
            }),
            NOW + 2,
        )
        .await
        .expect("override");
    let snapshot: EngineSnapshot = publisher.current();
    let override_state = snapshot.packs[0]
        .override_state
        .as_ref()
        .expect("the override is visible");
    assert_eq!(override_state.pack_id, "SN-0");
    assert!(override_state.active(NOW + 2));
}

#[tokio::test]
async fn an_unreachable_pack_is_a_fault() {
    let mut reading = pack("SN-0", 50.0);
    reading.available = false;
    reading.stale = true;
    let ledger = fakes::ledger_with(&["SN-0"]);
    let engine = Engine::new(
        EngineConfig::default(),
        Arc::new(fakes::FakeFleet::new(vec![reading])),
        Arc::new(fakes::FakeMeter::default()),
        Arc::new(fakes::FakeSource::new(plan(NOW, 0.0, 0.96))),
        Arc::new(fakes::FakeSink::new()),
        Arc::clone(&ledger),
    );
    let publisher = Arc::new(WatchPublisher::empty());
    let engine = Arc::new(engine.with_stream(
        Arc::clone(&publisher) as Arc<dyn SnapshotPublisher>,
        Arc::new(View),
    ));
    engine.boot(NOW).await.expect("boot");
    engine.tick_once(NOW).await.expect("tick");
    let snapshot = publisher.current();
    assert!(
        snapshot
            .faults
            .iter()
            .any(|fault| fault.kind == "pack_unreachable"),
        "a dead pack must open a fault: {:?}",
        snapshot.faults
    );
    assert!(snapshot.packs[0].stale);
}

//! A ready-to-run engine over the fakes, plus the ledger it writes to.

#![allow(dead_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_possible_wrap
)]

use std::sync::Arc;

use energy_core::engine::{Engine, EngineConfig};
use energy_core::plan::Plan;
use energy_core::safety::Authority;
use energy_ledger::Ledger;

use super::fakes::{self, FakeFleet, FakeMeter, FakeSink, FakeSource};

/// An engine and every port it talks to, so a test can script and inspect each one.
pub struct Harness {
    pub engine: Arc<Engine>,
    pub sink: Arc<FakeSink>,
    pub fleet: Arc<FakeFleet>,
    pub meter: Arc<FakeMeter>,
    pub source: Arc<FakeSource>,
    pub ledger: Arc<Ledger>,
}

/// A harness on a fresh in-memory ledger seeded with one pack row per reading.
pub fn build(
    mode: Authority,
    packs: Vec<energy_core::dispatch::PackReading>,
    plan: Plan,
) -> Harness {
    let ids: Vec<&str> = packs
        .iter()
        .map(energy_core::dispatch::PackReading::pack_id)
        .collect();
    build_on(fakes::ledger_with(&ids), mode, packs, plan)
}

/// A harness on an existing ledger (a restart, or a pre-seeded database).
pub fn build_on(
    ledger: Arc<Ledger>,
    mode: Authority,
    packs: Vec<energy_core::dispatch::PackReading>,
    plan: Plan,
) -> Harness {
    build_config(
        EngineConfig {
            mode,
            ..EngineConfig::default()
        },
        ledger,
        packs,
        plan,
    )
}

/// A harness with an explicit configuration (a shorter tick interval, other ceilings, ...).
pub fn build_config(
    config: EngineConfig,
    ledger: Arc<Ledger>,
    packs: Vec<energy_core::dispatch::PackReading>,
    plan: Plan,
) -> Harness {
    let fleet = Arc::new(FakeFleet::new(packs));
    let meter = Arc::new(FakeMeter::default());
    let sink = Arc::new(FakeSink::new());
    let source = Arc::new(FakeSource::new(plan));
    let engine = Arc::new(Engine::new(
        config,
        fleet.clone(),
        meter.clone(),
        source.clone(),
        sink.clone(),
        Arc::clone(&ledger),
    ));
    Harness {
        engine,
        sink,
        fleet,
        meter,
        source,
        ledger,
    }
}

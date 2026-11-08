//! Assembling the control engine from the ports this process actually has.
//!
//! Everything here is wiring: the fleet (reads + writes), the meter, the live plan source (HA
//! statistics + the price store + the learned load profile) and the ledger. The engine owns every
//! decision; this module must not.

use std::sync::Arc;

use energy_core::dispatch::{BatteryFleet, BatterySink, CommandSink, DeviceFleet, PackCatalog};
use energy_core::engine::{Engine, EngineConfig, PlanSource};
use energy_core::plan_source::{HaPlanSupply, LivePlanSource, LoadProfiles};
use energy_core::wiring::LedgerLearningStore;
use energy_ha::Connection;
use energy_ledger::Ledger;
use energy_price::{PriceRepository, Settlement};

use crate::bootstrap::fleet::Fleet;

/// The four ports one tick talks to.
pub struct Ports {
    pub fleet: Arc<dyn DeviceFleet>,
    pub sink: Arc<dyn CommandSink>,
    pub meter: crate::bootstrap::meter::MeterPort,
    pub source: Arc<dyn PlanSource>,
}

/// The fleet's read and write ports, with one ledger pack per device.
pub fn fleet_ports(
    ledger: &Arc<Ledger>,
    fleet: &Fleet,
    stale_after_s: f64,
) -> (Arc<dyn DeviceFleet>, Arc<dyn CommandSink>) {
    let catalog = Arc::new(PackCatalog::new(
        Arc::clone(ledger),
        Some(Arc::new(LedgerLearningStore::new(Arc::clone(ledger)))),
    ));
    (
        Arc::new(BatteryFleet::new(
            Arc::clone(&fleet.client),
            catalog,
            stale_after_s,
        )),
        Arc::new(BatterySink::new(fleet.controllers.clone())),
    )
}

/// The live plan source over the shared HA socket, the price store and the learned profile.
pub fn plan_source(
    connection: std::sync::Arc<Connection>,
    prices: Arc<dyn PriceRepository>,
    settlement: Settlement,
    profiles: Arc<LoadProfiles>,
    policy: energy_core::plan::Policy,
) -> Arc<dyn PlanSource> {
    let config = energy_core::plan_source::SourceConfig {
        policy,
        wear_eur_per_kwh: policy.wear_eur_per_kwh,
        ..energy_core::plan_source::SourceConfig::default()
    };
    Arc::new(LivePlanSource::new(
        HaPlanSupply::new(connection, prices, settlement).with_load_profiles(profiles),
        config,
    ))
}

/// Build the engine over the assembled ports (the caller attaches the API stream).
pub fn engine(ledger: Arc<Ledger>, config: EngineConfig, ports: Ports) -> Engine {
    Engine::new(
        config,
        ports.fleet,
        ports.meter,
        ports.source,
        ports.sink,
        ledger,
    )
}

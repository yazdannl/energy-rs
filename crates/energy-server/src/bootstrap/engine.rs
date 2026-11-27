//! Assembling the control engine from the ports this process actually has.
//!
//! Everything here is wiring: the fleet (reads + writes), the meter, the live plan source (HA
//! statistics + the price store + the learned load profile) and the ledger. The engine owns every
//! decision; this module must not.

use std::sync::Arc;
use std::time::Duration;

use energy_core::dispatch::{BatteryFleet, BatterySink, CommandSink, DeviceFleet, PackCatalog};
use energy_core::engine::{Engine, EngineConfig, PlanSource};
use energy_core::maintenance::{FullChargeHistory, LedgerFullChargeHistory, MaintenancePort};
use energy_core::plan::Policy;
use energy_core::plan_source::{HaPlanSupply, LivePlanSource, PolicyHandle, SourceConfig};
use energy_core::wiring::LedgerLearningStore;
use energy_ledger::Ledger;

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
        Arc::new(BatterySink::new(
            fleet.controllers.clone(),
            Duration::from_secs_f64(stale_after_s.max(0.0)),
        )),
    )
}

/// The plan-side policy from the engine's operator settings.
///
/// The hard `reserve_soc_pct` must be the same number the planner uses for `e_min_kwh` and the
/// clamp ladder uses to stop a discharge. The policy's peak and solar reserves are different:
/// the first is a soft end-of-horizon guideline, the second is headroom kept free of grid energy;
/// neither lowers or raises this floor.
pub fn source_config(engine: &EngineConfig, policy: Policy) -> SourceConfig {
    SourceConfig {
        reserve_soc_pct: engine.reserve_soc_pct,
        wear_eur_per_kwh: policy.wear_eur_per_kwh,
        load_forecast_bias_slots: engine.load_forecast_bias_slots,
        load_forecast_bias_lookback_s: engine.load_forecast_bias_lookback_s,
        policy,
        ..SourceConfig::default()
    }
}

/// The live plan source over an assembled supply, with the boot config and live policy handle.
pub fn plan_source(
    supply: HaPlanSupply,
    config: SourceConfig,
    handle: Arc<PolicyHandle>,
) -> LivePlanSource<HaPlanSupply> {
    LivePlanSource::new(supply, config).with_policy(handle)
}

/// The shared maintenance port: ledger-backed history plus the resolved settings.
pub fn maintenance_port(ledger: &Arc<Ledger>, config: &EngineConfig) -> Arc<MaintenancePort> {
    let history: Arc<dyn FullChargeHistory> =
        Arc::new(LedgerFullChargeHistory::new(Arc::clone(ledger)));
    Arc::new(MaintenancePort::new(history, config.full_charge))
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

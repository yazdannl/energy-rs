//! Startup assembly: stores, Home Assistant, batteries, the engine and the background loops.

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use energy_battery::control::ControlLimits;
use energy_core::engine::Engine;
use energy_core::plan_source::LoadProfiles;
use energy_core::safety::Authority;
use energy_core::stream::RuntimeView;
use energy_ledger::{Ledger, LedgerConfig};

use crate::bootstrap::booted::Booted;
use crate::bootstrap::engine::Ports;
use crate::bootstrap::site_cache::SiteCache;
use crate::bootstrap::{
    config, engine, fleet, ha, ha_loop, meter, options, prices, profile, savings,
};
use crate::service::Service;
use crate::service_options::ServiceOptions;

/// Everything `boot` assembles before the engine exists.
pub(crate) struct Startup {
    ledger: Arc<Ledger>,
    cache: Arc<SiteCache>,
    prices: Arc<prices::Prices>,
    profiles: Arc<LoadProfiles>,
    ha: ha::Ha,
    fleet: fleet::Fleet,
}

impl Startup {
    /// Open the stores, resolve HA, learn the profile and discover the batteries.
    pub(crate) async fn open(options: &options::Options) -> Result<Self> {
        std::fs::create_dir_all(options.data_dir()).with_context(|| {
            format!(
                "create the data folder {} (is /data mounted?)",
                options.data_dir().display()
            )
        })?;
        let ledger = Arc::new(
            Ledger::open(&LedgerConfig::at(options.database())).context("open the ledger")?,
        );
        let cache = Arc::new(SiteCache::new());
        let ha = ha::Ha::connect(options, Arc::clone(&cache)).await?;
        let prices = Arc::new(prices::Prices::open(options, Arc::clone(&cache))?);
        prices.poll_once().await;
        let profiles = profile::store(Arc::clone(&ledger));
        profile::refresh_once(&ha.client(), &profiles, &cache, Utc::now().timestamp()).await;
        let fleet = fleet::build(options, control_limits(&ledger)).await?;
        Ok(Self {
            ledger,
            cache,
            prices,
            profiles,
            ha,
            fleet,
        })
    }

    /// Build the engine, the service and the background loops.
    pub(crate) async fn assemble(self, options: &options::Options) -> Result<Booted> {
        let Self {
            ledger,
            cache,
            prices,
            profiles,
            ha,
            fleet,
        } = self;
        let engine_config = config::engine_config(options, &ledger);
        let engine = wire_engine(&ledger, &fleet, &ha, &prices, &profiles, engine_config).await?;
        let (service, handles) = Service::new(
            Arc::clone(&ledger),
            Arc::clone(&prices.repository),
            prices.settlement(),
            ServiceOptions::default(),
        );
        let runtime: Arc<dyn RuntimeView> = Arc::clone(&cache) as Arc<dyn RuntimeView>;
        let engine = Arc::new(engine.with_stream(Arc::new(handles.snapshots.clone()), runtime));
        engine
            .boot(Utc::now().timestamp())
            .await
            .context("the engine could not boot (check the ledger and the device configuration)")?;

        let tasks = spawn_loops(&ha, &prices, &profiles, &cache, &service);
        let engine_task = spawn_engine(&engine, handles.commands);
        Summary {
            devices: fleet.controllers.len(),
            manual: fleet.manual,
            discovered: fleet.discovered,
            dashboard: cache.site().configured,
            prices: prices.provider_name(),
            profile_days: profiles
                .current(Utc::now().timestamp())
                .map_or(0, |profile| profile.days),
            mode: engine.mode()?,
        }
        .log();

        Ok(Booted {
            service,
            engine,
            ledger,
            cache,
            address: options.address(),
            engine_task: Some(engine_task),
            tasks,
        })
    }
}

/// The control engine over the real fleet, meter and plan source.
async fn wire_engine(
    ledger: &Arc<Ledger>,
    fleet: &fleet::Fleet,
    ha: &ha::Ha,
    prices: &Arc<prices::Prices>,
    profiles: &Arc<LoadProfiles>,
    config: energy_core::engine::EngineConfig,
) -> Result<Engine> {
    let (fleet_port, sink) = engine::fleet_ports(ledger, fleet, config.max_state_age_s);
    let meter = meter::HaMeter::new(ha.client(), Arc::clone(&ha.site()));
    let connection = ha
        .client()
        .connection()
        .await
        .context("Home Assistant connection lost during startup")?;
    let source = engine::plan_source(
        connection,
        Arc::clone(&prices.repository),
        prices.settlement(),
        Arc::clone(profiles),
    );
    Ok(engine::engine(
        Arc::clone(ledger),
        config,
        Ports {
            fleet: fleet_port,
            sink,
            meter: meter.port(),
            source,
        },
    ))
}

/// Run the engine's tick loop, draining the API command queue between ticks.
fn spawn_engine(
    engine: &Arc<Engine>,
    commands: tokio::sync::mpsc::Receiver<energy_core::stream::EngineCommand>,
) -> tokio::task::JoinHandle<std::result::Result<(), energy_core::engine::EngineError>> {
    let runner = Arc::clone(engine);
    tokio::spawn(async move { runner.run_with(Some(commands)).await })
}

/// Start the price, profile, discovery and link loops.
fn spawn_loops(
    ha: &ha::Ha,
    prices: &Arc<prices::Prices>,
    profiles: &Arc<LoadProfiles>,
    cache: &Arc<SiteCache>,
    service: &Arc<Service>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut tasks = vec![
        ha.watch_link(|| Utc::now().timestamp()),
        prices.spawn(),
        profile::spawn(
            ha.client(),
            Arc::clone(profiles),
            Arc::clone(cache),
            service.profile_refresh(),
        ),
        ha_loop::spawn(ha.client(), ha.site(), Arc::clone(cache)),
        savings::spawn(ha, prices, service),
    ];
    tasks.retain(|task| !task.is_finished());
    tasks
}

/// Read the options, assemble everything and start the loops.
pub async fn boot(options: options::Options) -> Result<Booted> {
    Startup::open(&options).await?.assemble(&options).await
}

/// The control limit for each device: the operator's aggregate cap, or the device's own limits.
#[allow(clippy::cast_sign_loss)] // filtered to a finite positive value above the cast
fn control_limits(ledger: &Ledger) -> ControlLimits {
    let limit = ledger
        .settings()
        .and_then(|settings| settings.get("battery_power_limit_w"))
        .ok()
        .flatten()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0);
    match limit {
        Some(watts) => ControlLimits::new(watts as u32, watts as u32),
        None => ControlLimits::unlimited(),
    }
}

/// The one-line startup summary: counts and names, never a token or a path.
struct Summary {
    devices: usize,
    manual: usize,
    discovered: usize,
    dashboard: bool,
    prices: &'static str,
    profile_days: usize,
    mode: Authority,
}

impl Summary {
    fn log(&self) {
        tracing::info!(
            batteries = self.devices,
            manual = self.manual,
            mdns = self.discovered,
            dashboard = self.dashboard,
            prices = self.prices,
            profile_days = self.profile_days,
            mode = self.mode.slug(),
            "energy-rs ready"
        );
    }
}

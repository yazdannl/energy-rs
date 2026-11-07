//! The grid meter: the dashboard's live power entities, read once per tick at most.
//!
//! The reading is the site balance (`energy-ha`'s `SiteModel::live`), never a raw sensor: grid,
//! PV and battery power arrive as W and the house load is the balance, so the ladder's `grid_w` and
//! the forecast's base agree on the same physics. A failed read is an empty reading (`None` fields),
//! which the ladder treats as missing telemetry, never as zero.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use energy_core::dispatch::{BoxFuture, MeterReading, MeterSource};
use energy_ha::{EntityState, HaClient, PowerReading, SiteModel};

/// How long one reading is reused; a tick every 5 s must not become two REST calls.
const CACHE: Duration = Duration::from_secs(4);

/// A [`MeterSource`] over the discovery model and the REST states endpoint.
#[derive(Clone)]
pub struct HaMeter {
    client: Arc<HaClient>,
    site: Arc<RwLock<SiteModel>>,
    cached: Arc<Mutex<Option<(Instant, MeterReading)>>>,
}

impl HaMeter {
    /// A meter over one client and the shared dashboard model.
    pub fn new(client: Arc<HaClient>, site: Arc<RwLock<SiteModel>>) -> Self {
        Self {
            client,
            site,
            cached: Arc::new(Mutex::new(None)),
        }
    }

    /// The port form the engine stores.
    pub fn port(&self) -> Arc<dyn MeterSource> {
        Arc::new(self.clone())
    }

    /// Read the states and map them onto the dashboard's power entities.
    async fn read(&self) -> MeterReading {
        let states = match self.client.states().await {
            Ok(states) => states,
            Err(error) => {
                tracing::warn!(%error, "meter read failed; treating grid power as unknown");
                return MeterReading::default();
            }
        };
        let readings = power_readings(&states);
        let site = self
            .site
            .read()
            .map(|site| site.clone())
            .unwrap_or_default();
        let live = site.live(&readings);
        MeterReading {
            grid_w: live.grid_w,
            load_kw: live.load_w.map(|watts| watts / 1000.0),
            pv_kw: live.pv_w.map(|watts| watts / 1000.0),
        }
    }
}

impl MeterSource for HaMeter {
    fn reading(&self) -> BoxFuture<'_, MeterReading> {
        Box::pin(async move {
            if let Ok(cached) = self.cached.lock()
                && let Some((at, reading)) = cached.as_ref()
                && at.elapsed() < CACHE
            {
                return *reading;
            }
            let reading = self.read().await;
            if let Ok(mut cached) = self.cached.lock() {
                *cached = Some((Instant::now(), reading));
            }
            reading
        })
    }
}

/// Every state that carries a usable power value.
fn power_readings(states: &[EntityState]) -> HashMap<String, PowerReading> {
    let mut readings = HashMap::new();
    for state in states {
        if let Some(reading) = PowerReading::from_state(state) {
            readings.insert(state.entity_id.clone(), reading);
        }
    }
    readings
}

/// The boxed meter the engine holds.
pub type MeterPort = Arc<dyn MeterSource>;

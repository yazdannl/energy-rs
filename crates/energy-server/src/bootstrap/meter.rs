//! The grid meter: the dashboard's live power entities, read once per tick at most.
//!
//! Only the entities that can contribute a power reading are fetched — a handful of targeted reads
//! rather than the whole state machine — so a one-second tick does not become one full `/states`
//! per second. A dashboard that exposes no power entity falls back to the broad read.
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

/// How long one reading is reused: exactly one tick, so a 1 s loop never gets 4 s old numbers.
const DEFAULT_CACHE: Duration = Duration::from_secs(1);

/// A [`MeterSource`] over the discovery model and the REST states endpoint.
#[derive(Clone)]
pub struct HaMeter {
    client: Arc<HaClient>,
    site: Arc<RwLock<SiteModel>>,
    cached: Arc<Mutex<Option<(Instant, MeterReading)>>>,
    cache: Duration,
}

impl HaMeter {
    /// A meter over one client and the shared dashboard model.
    pub fn new(client: Arc<HaClient>, site: Arc<RwLock<SiteModel>>) -> Self {
        Self {
            client,
            site,
            cached: Arc::new(Mutex::new(None)),
            cache: DEFAULT_CACHE,
        }
    }

    /// Reuse a reading for this long; the boot code passes the tick interval.
    #[must_use]
    pub fn with_cache(mut self, cache: Duration) -> Self {
        self.cache = cache.max(Duration::from_millis(200));
        self
    }

    /// The port form the engine stores.
    pub fn port(&self) -> Arc<dyn MeterSource> {
        Arc::new(self.clone())
    }

    /// Read the power entities and map them onto the dashboard's model.
    ///
    /// Targeted reads when the dashboard names its power entities, the broad read when it does not:
    /// the point of the narrow path is that a 1 s tick stays a few kB, not a full state dump.
    async fn read(&self) -> MeterReading {
        let site = self
            .site
            .read()
            .map(|site| site.clone())
            .unwrap_or_default();
        let Some(states) = self.states(&site).await else {
            return MeterReading::default();
        };
        let readings = power_readings(&states);
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
                && at.elapsed() < self.cache
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

impl HaMeter {
    /// The power states this tick, or `None` when the read failed.
    async fn states(&self, site: &SiteModel) -> Option<Vec<EntityState>> {
        let ids = site.power_entity_ids();
        if ids.is_empty() {
            return self.broad_states().await;
        }
        self.targeted_states(&ids).await
    }

    /// The broad read, for a dashboard that names no power entity.
    async fn broad_states(&self) -> Option<Vec<EntityState>> {
        match self.client.states().await {
            Ok(states) => Some(states),
            Err(error) => {
                tracing::warn!(%error, "meter read failed; treating grid power as unknown");
                None
            }
        }
    }

    /// One small read per configured power entity.
    async fn targeted_states(&self, ids: &[String]) -> Option<Vec<EntityState>> {
        let mut states = Vec::with_capacity(ids.len());
        for id in ids {
            match self.client.state(id).await {
                Ok(Some(state)) => states.push(state),
                // A missing entity is simply absent: the balance reports the missing term and the
                // ladder fails closed on it.
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, entity = %id, "meter entity read failed");
                    return None;
                }
            }
        }
        Some(states)
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

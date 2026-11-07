//! Re-resolving the dashboard in the background.
//!
//! The dashboard changes rarely, but when it does every statistic mapping changes with it, so the
//! model is refreshed on a slow interval rather than once per boot. A failure keeps the previous
//! model and opens the `ha_disconnected` fault: stale discovery is better than no discovery.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use energy_ha::{HaClient, SiteModel};

use crate::bootstrap::site_cache::{SiteCache, site_snapshot};

/// Dashboard refresh cadence.
const REDISCOVER: Duration = Duration::from_secs(600);

/// Refresh the dashboard model every ten minutes.
pub fn spawn(
    client: Arc<HaClient>,
    site: Arc<RwLock<SiteModel>>,
    cache: Arc<SiteCache>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(REDISCOVER);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let Ok(connection) = client.connection().await else {
                tracing::warn!("dashboard rediscovery skipped: no Home Assistant connection");
                continue;
            };
            match energy_ha::discover(&connection).await {
                Ok(model) => {
                    cache.set_site(site_snapshot(&model));
                    if let Ok(mut current) = site.write() {
                        *current = model;
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "dashboard rediscovery failed; keeping the previous model");
                }
            }
        }
    })
}

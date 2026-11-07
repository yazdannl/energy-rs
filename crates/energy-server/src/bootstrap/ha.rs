//! The Home Assistant link: one supervised WebSocket, used for discovery and the meter.
//!
//! Add-on mode always goes through the Supervisor proxy (`http://supervisor/core`) with
//! `SUPERVISOR_TOKEN`; direct mode takes `HASS_URL` + `HASS_TOKEN`. The token never appears in a log
//! line: `energy-ha` redacts it from every error, and this module only ever formats discovery
//! counts.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use energy_ha::{Config, HaClient, LinkState, SiteModel, discover};

use crate::bootstrap::options::Options;
use crate::bootstrap::site_cache::{SiteCache, site_snapshot};

/// How long startup waits for the WebSocket handshake before failing fast.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// The HA client plus the dashboard it last discovered.
pub struct Ha {
    client: Arc<HaClient>,
    site: Arc<RwLock<SiteModel>>,
    cache: Arc<SiteCache>,
}

impl Ha {
    /// Connect, wait for the socket and resolve the dashboard once.
    pub async fn connect(options: &Options, cache: Arc<SiteCache>) -> Result<Self> {
        let (supervisor, url, token) = options.ha()?;
        let config = if supervisor {
            Config::supervisor(token)
        } else {
            Config::direct(url, token).context("Home Assistant URL is unusable")?
        };
        let client = HaClient::new(config).context("Home Assistant client cannot start")?;
        tokio::time::timeout(CONNECT_TIMEOUT, client.connection())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Home Assistant did not accept the WebSocket within {}s; check the URL and the token",
                    CONNECT_TIMEOUT.as_secs()
                )
            })?
            .context("Home Assistant WebSocket handshake failed")?;
        let connection = client.connection().await?;
        let site = discover(&connection)
            .await
            .context("Home Assistant discovery failed")?;
        cache.set_site(site_snapshot(&site));
        let area = Arc::new(RwLock::new(site));
        Ok(Self {
            client: Arc::new(client),
            site: area,
            cache,
        })
    }

    /// The client (meter, prices, plan supply all share one supervisor).
    pub fn client(&self) -> Arc<HaClient> {
        Arc::clone(&self.client)
    }

    /// The current dashboard model.
    pub fn site(&self) -> Arc<RwLock<SiteModel>> {
        Arc::clone(&self.site)
    }

    /// The last discovered model.
    pub fn current_site(&self) -> SiteModel {
        self.site
            .read()
            .map(|site| site.clone())
            .unwrap_or_default()
    }

    /// The discovery cache (the snapshot's site/fault half).
    pub fn cache(&self) -> Arc<SiteCache> {
        Arc::clone(&self.cache)
    }

    /// Re-resolve the dashboard and refresh the cache.
    pub async fn rediscover(&self) -> Result<SiteModel> {
        let connection = self.client.connection().await?;
        let site = discover(&connection)
            .await
            .context("Home Assistant discovery failed")?;
        self.cache.set_site(site_snapshot(&site));
        if let Ok(mut current) = self.site.write() {
            *current = site.clone();
        }
        Ok(site)
    }

    /// Follow the supervised link and keep the `ha_disconnected` fault current.
    pub fn watch_link(
        &self,
        now: impl Fn() -> i64 + Send + 'static,
    ) -> tokio::task::JoinHandle<()> {
        let mut state = self.client.supervisor().state();
        let cache = Arc::clone(&self.cache);
        tokio::spawn(async move {
            loop {
                if state.changed().await.is_err() {
                    return;
                }
                match *state.borrow() {
                    LinkState::Connected => cache.set_fault("ha_disconnected", None, now()),
                    LinkState::Connecting | LinkState::Disconnected => cache.set_fault(
                        "ha_disconnected",
                        Some("the Home Assistant WebSocket is not connected".to_owned()),
                        now(),
                    ),
                }
            }
        })
    }
}

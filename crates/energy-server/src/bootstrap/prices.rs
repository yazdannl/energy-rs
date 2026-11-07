//! The price store and provider: fetch once per publication, never per tick.
//!
//! `auto` prefers the direct Zonneplan API when a refresh token is configured and falls back to the
//! integration's tariff sensor otherwise. Either way the fetched book is saved through
//! `PriceRepository`, which is idempotent by content hash, so a repeated poll writes nothing.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use energy_price::book::query::local_date_of;
use energy_price::{
    Contract, HaCredentials, HaSensorFallback, PriceError, PriceProvider, PriceQuery,
    PriceRepository, SaveOutcome, Settlement, SqlitePriceRepository, ZonneplanCredentials,
    ZonneplanDirect,
};
use tokio::time::MissedTickBehavior;

use crate::bootstrap::options::Options;
use crate::bootstrap::site_cache::SiteCache;

/// How often the publication is polled (a save that changes nothing is free).
const POLL: Duration = Duration::from_mins(15);
/// Retry delay after a failed poll.
const RETRY: Duration = Duration::from_mins(5);
/// How far ahead the planner wants prices, seconds.
const HORIZON_S: i64 = 36 * 3600;

/// The price store, the provider and the contract constants.
pub struct Prices {
    pub repository: Arc<dyn PriceRepository>,
    pub contract: Contract,
    provider: Option<Box<dyn PriceProvider>>,
    cache: Arc<SiteCache>,
}

impl Prices {
    /// Open the store and choose the provider the options asked for.
    pub fn open(options: &Options, cache: Arc<SiteCache>) -> Result<Self> {
        let repository: Arc<dyn PriceRepository> = Arc::new(
            SqlitePriceRepository::open(options.database()).context("open the price store")?,
        );
        let provider = provider(options)?;
        Ok(Self {
            repository,
            contract: Contract::zonneplan_2026(),
            provider,
            cache,
        })
    }

    /// The settlement the engine plans with.
    pub fn settlement(&self) -> Settlement {
        Settlement::new(self.contract.clone())
    }

    /// The provider's stable name, for the startup summary.
    pub fn provider_name(&self) -> &'static str {
        self.provider
            .as_ref()
            .map_or("store_only", |provider| provider.name())
    }

    /// Fetch and store one publication; `None` when no provider is configured.
    pub async fn refresh_once(&self, now: i64) -> Result<Option<SaveOutcome>, PriceError> {
        let Some(provider) = self.provider.as_ref() else {
            return Ok(None);
        };
        let query = query(now)?;
        let book = provider.fetch(query, &self.contract).await?;
        self.repository.save_publication(&book).map(Some)
    }

    /// Poll the provider for the life of the process.
    pub fn spawn(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let prices = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(POLL);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                if !prices.poll_once().await {
                    tokio::time::sleep(RETRY).await;
                }
            }
        })
    }

    /// One poll; the fault cache is kept current and `false` asks the loop to back off.
    ///
    /// A boot poll must return promptly even when the provider is down, so the retry delay lives in
    /// the loop, not here.
    pub async fn poll_once(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        match self.refresh_once(now).await {
            Ok(outcome) => {
                self.record_success(now, outcome.as_ref());
                true
            }
            Err(error) => {
                tracing::warn!(%error, provider = self.provider_name(), "price refresh failed");
                self.cache
                    .set_fault("prices_stale", Some(error.to_string()), now);
                false
            }
        }
    }

    fn record_success(&self, now: i64, outcome: Option<&SaveOutcome>) {
        if outcome.is_some() {
            tracing::info!(
                new = outcome.is_some_and(SaveOutcome::is_new),
                provider = self.provider_name(),
                "prices refreshed"
            );
        }
        self.cache.set_fault("prices_stale", None, now);
    }
}

/// A query for today plus tomorrow (tomorrow may not be published before ~13:00 CET).
fn query(now: i64) -> Result<PriceQuery, PriceError> {
    let today = local_date_of(now)
        .or_else(|| NaiveDate::from_ymd_opt(2026, 1, 1))
        .ok_or_else(|| PriceError::Payload("cannot resolve the local price date".to_owned()))?;
    let tomorrow = today.succ_opt().unwrap_or(today);
    PriceQuery::new(today, tomorrow, now, HORIZON_S)
}

/// Construct the configured provider.
fn provider(options: &Options) -> Result<Option<Box<dyn PriceProvider>>> {
    let (choice, direct, sensor) = options.price_choice();
    if direct {
        let Some(token) = options.refresh_token() else {
            if choice == "zonneplan" {
                bail!("price_provider is zonneplan but no zonneplan_refresh_token is configured");
            }
            return Ok(None);
        };
        let credentials = ZonneplanCredentials::new(token)
            .context("the configured Zonneplan refresh token is unusable")?;
        let provider =
            ZonneplanDirect::new(credentials).context("Zonneplan provider cannot start")?;
        return Ok(Some(Box::new(provider)));
    }
    if sensor {
        return Ok(Some(Box::new(ha_provider(options)?)));
    }
    Ok(None)
}

/// The HA sensor fallback over the same transport as the engine.
fn ha_provider(options: &Options) -> Result<HaSensorFallback> {
    let (_, url, token) = options.ha()?;
    let credentials = HaCredentials::new(url, token)
        .context("the HA price fallback has no usable credentials")?;
    let mut provider =
        HaSensorFallback::new(credentials).context("the HA price fallback cannot start")?;
    if let Some(entity) = options.price_entity() {
        provider = provider.with_entities(vec![entity.to_owned()]);
    }
    Ok(provider)
}

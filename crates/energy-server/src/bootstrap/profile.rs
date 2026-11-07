//! Learning and refreshing the house load profile.
//!
//! The profile is learned from the energy dashboard's hourly history at boot and once a day, and
//! again whenever a setting changes (the API nudges [`tokio::sync::Notify`]). A failure is a fault in
//! the snapshot and a logged warning, never a reason to stop the engine: without a profile the plan
//! falls back to the residential shape and says so.

use std::sync::Arc;
use std::time::Duration;

use energy_core::plan_source::{LoadProfiles, learn_load_profile};
use energy_ha::HaClient;
use energy_ledger::Ledger;
use tokio::sync::Notify;

use crate::bootstrap::site_cache::SiteCache;

/// Seconds a learned profile is used before it is replaced (daily, with an hour of slack).
const REFRESH: Duration = Duration::from_hours(24);
/// Silence for this long means the profile refresh is stuck; the next tick tries again.
const MIN_GAP: Duration = Duration::from_mins(30);

/// Build the profile store over the ledger.
pub fn store(ledger: Arc<Ledger>) -> Arc<LoadProfiles> {
    Arc::new(LoadProfiles::new(ledger))
}

/// Learn once and report the outcome in the fault cache.
pub async fn refresh_once(client: &HaClient, profiles: &LoadProfiles, cache: &SiteCache, now: i64) {
    let Some(connection) = connect(client, cache, now).await else {
        return;
    };
    let result = learn_load_profile(&connection, now, profiles).await;
    record(cache, now, result);
}

/// The current HA socket, or a fault when there is none.
async fn connect(
    client: &HaClient,
    cache: &SiteCache,
    now: i64,
) -> Option<std::sync::Arc<energy_ha::Connection>> {
    match client.connection().await {
        Ok(connection) => Some(connection),
        Err(error) => {
            tracing::warn!(%error, "load profile refresh skipped: no Home Assistant connection");
            cache.set_fault(
                "load_profile",
                Some(
                    "the load profile could not be refreshed: Home Assistant is unreachable".into(),
                ),
                now,
            );
            None
        }
    }
}

/// Log the learning outcome and keep the fault current.
fn record(
    cache: &SiteCache,
    now: i64,
    result: Result<energy_forecast::load::LoadProfile, energy_core::plan::PlanError>,
) {
    match result {
        Ok(profile) => record_success(cache, now, &profile),
        Err(error) => record_failure(cache, now, &error),
    }
}

fn record_success(cache: &SiteCache, now: i64, profile: &energy_forecast::load::LoadProfile) {
    tracing::info!(
        days = profile.days,
        samples = profile.samples,
        "load profile learned"
    );
    cache.set_fault("load_profile", None, now);
}

fn record_failure(cache: &SiteCache, now: i64, error: &energy_core::plan::PlanError) {
    tracing::warn!(%error, "load profile learning failed; the plan uses the fallback");
    cache.set_fault(
        "load_profile",
        Some(format!("no learned load profile yet: {error}")),
        now,
    );
}

/// Refresh in the background: daily, and on the refresh signal from the API.
pub fn spawn(
    client: std::sync::Arc<HaClient>,
    profiles: Arc<LoadProfiles>,
    cache: Arc<SiteCache>,
    signal: Arc<Notify>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let now = chrono::Utc::now().timestamp();
            refresh_once(&client, &profiles, &cache, now).await;
            tokio::select! {
                () = tokio::time::sleep(REFRESH) => {},
                () = signal.notified() => {
                    tracing::info!("load profile refresh requested");
                    tokio::time::sleep(MIN_GAP).await;
                }
            }
        }
    })
}

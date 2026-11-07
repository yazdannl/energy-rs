use std::{collections::BTreeSet, sync::Arc, time::Duration};

use chrono::{Duration as ChronoDuration, Utc};
use energy_ha::{
    Config, Connection, HaClient, SiteModel, discover, slot_series_15min, statistics_metadata,
    watch_entities,
};

async fn bootstrap() -> (HaClient, Arc<Connection>) {
    let client = HaClient::new(Config::from_env().expect("HASS_URL/HASS_TOKEN")).expect("client");
    let connection = tokio::time::timeout(Duration::from_secs(20), client.connection())
        .await
        .expect("timeout")
        .expect("connection");
    (client, connection)
}

fn statistic_ids(site: &SiteModel) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for entity in [&site.grid.import, &site.grid.export, &site.solar.energy]
        .into_iter()
        .flatten()
        .chain(site.batteries.iter().flat_map(|battery| {
            [battery.discharge.as_ref(), battery.charge.as_ref()]
                .into_iter()
                .flatten()
        }))
    {
        ids.insert(entity.clone());
    }
    ids.into_iter().collect()
}

async fn slot_counts(connection: &Connection, site: &SiteModel) -> (usize, usize) {
    let Some(import) = &site.grid.import else {
        return (0, 0);
    };
    let to = Utc::now();
    let from = to - ChronoDuration::hours(2);
    let series = slot_series_15min(connection, import, from, to)
        .await
        .expect("slot series");
    (
        series.len(),
        series.iter().filter(|slot| slot.is_gap()).count(),
    )
}

async fn snapshot_size(connection: &Connection, site: &SiteModel) -> usize {
    let live_ids: Vec<String> = site
        .grid
        .power
        .iter()
        .flat_map(|rule| rule.entities())
        .chain(site.solar.power.iter().flat_map(|rule| rule.entities()))
        .chain(
            site.batteries
                .iter()
                .filter_map(|battery| battery.soc.as_deref()),
        )
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let watch = watch_entities(connection, &live_ids, Vec::new())
        .await
        .expect("subscription");
    watch.snapshot.len()
}

#[tokio::test]
#[ignore = "requires HASS_URL/HASS_TOKEN and read-only access to a live Home Assistant"]
async fn live_read_only_smoke() {
    let (client, connection) = bootstrap().await;
    let core = tokio::time::timeout(Duration::from_secs(20), client.core_config())
        .await
        .expect("timeout")
        .expect("core config");
    let site = tokio::time::timeout(Duration::from_secs(20), discover(&connection))
        .await
        .expect("timeout")
        .expect("discover");
    assert!(
        site.configured,
        "this smoke needs a configured energy dashboard"
    );

    let ids = statistic_ids(&site);
    let metadata = statistics_metadata(&connection, &ids)
        .await
        .expect("metadata");
    let (slots, gaps) = slot_counts(&connection, &site).await;
    let snapshot = snapshot_size(&connection, &site).await;

    println!(
        "ha {} tz={} currency={} batteries={} metadata={} slots={} gaps={} snapshot={}",
        core.version,
        core.time_zone,
        core.currency,
        site.batteries.len(),
        metadata.len(),
        slots,
        gaps,
        snapshot,
    );
}

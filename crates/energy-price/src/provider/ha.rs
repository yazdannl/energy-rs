//! Fallback provider: the Zonneplan integration's HA sensor forecast over the HA REST API.
//! Staleness is decided from coverage and timestamps -- the entity stays "available" with an old
//! payload, so `unavailable` is never the signal.  The gas tariff sensor is refused by unit/grid.

use std::time::Duration;

use serde_json::Value;

use crate::book::{PriceBook, PriceQuery};
use crate::error::PriceError;
use crate::provider::ha_credentials::HaCredentials;
use crate::provider::parse::{ProviderKind, ingest};
use crate::provider::parse_ha::parse_state;
use crate::provider::price_provider::{BoxFuture, PriceProvider};
use crate::settlement::Contract;

const REQUEST_TIMEOUT_SECS: u64 = 15;

/// HA sensor fallback provider.
pub struct HaSensorFallback {
    client: reqwest::Client,
    credentials: HaCredentials,
    entities: Vec<String>,
}

impl HaSensorFallback {
    /// Quarter-hourly sensor, enabled by default in the integration.
    pub const QUARTER_ENTITY: &'static str =
        "sensor.zonneplan_current_quarter_hourly_electricity_tariff";
    /// Legacy hourly sensor, disabled by default; tried second.
    pub const HOUR_ENTITY: &'static str = "sensor.zonneplan_current_electricity_tariff";

    /// Provider with the two known Zonneplan tariff entities, quarter-hourly first.
    pub fn new(credentials: HaCredentials) -> Result<Self, PriceError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .user_agent("energy-rs")
            .build()
            .map_err(|err| PriceError::Http {
                url: credentials.base_url().to_owned(),
                reason: err.to_string(),
            })?;
        Ok(Self {
            client,
            credentials,
            entities: vec![
                Self::QUARTER_ENTITY.to_owned(),
                Self::HOUR_ENTITY.to_owned(),
            ],
        })
    }

    /// Replace the sensor preference list.
    pub fn with_entities(mut self, entities: Vec<String>) -> Self {
        self.entities = entities;
        self
    }

    async fn fetch_entity(
        &self,
        entity: &str,
        query: PriceQuery,
        contract: &Contract,
    ) -> Result<PriceBook, PriceError> {
        let url = format!(
            "{}/api/states/{entity}",
            self.credentials.base_url().trim_end_matches('/')
        );
        let response = self
            .client
            .get(&url)
            .bearer_auth(self.credentials.token())
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|err| PriceError::Http {
                url: url.clone(),
                reason: err.to_string(),
            })?;
        let status = response.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(PriceError::Auth(format!(
                "HA state {entity} refused with {status}"
            )));
        }
        if status.as_u16() == 404 {
            return Err(PriceError::Payload(format!("entity {entity} not found")));
        }
        if !status.is_success() {
            return Err(PriceError::Http {
                url,
                reason: format!("status {status}"),
            });
        }
        let payload: Value = response.json().await.map_err(|err| PriceError::Http {
            url,
            reason: format!("response is not JSON: {err}"),
        })?;
        let series = parse_state(&payload)?;
        let book = ingest(series, ProviderKind::Ha, contract, &query)?;
        // Only `now` must be covered: before ~13:00 CET tomorrow is not published, and the caller
        // shortens its horizon instead of the provider inventing slots (prices.md §1.4, §4.7).
        book.check_coverage(query.now(), 0)?;
        Ok(book)
    }
}

impl PriceProvider for HaSensorFallback {
    fn name(&self) -> &'static str {
        "ha_sensor"
    }

    fn fetch<'a>(
        &'a self,
        query: PriceQuery,
        contract: &'a Contract,
    ) -> BoxFuture<'a, Result<PriceBook, PriceError>> {
        Box::pin(async move {
            let mut stale: Option<PriceError> = None;
            let mut last: Option<PriceError> = None;
            for entity in &self.entities {
                match self.fetch_entity(entity, query, contract).await {
                    Ok(book) => return Ok(book),
                    Err(error @ PriceError::Auth(_)) => return Err(error),
                    Err(error @ PriceError::Stale { .. }) => stale = Some(error),
                    Err(error) => {
                        tracing::warn!(entity, error = %error, "HA price sensor unusable");
                        last = Some(error);
                    }
                }
            }
            Err(stale.or(last).unwrap_or(PriceError::NotPublished {
                from: query.from(),
                to: query.to(),
            }))
        })
    }
}

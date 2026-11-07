//! Direct Zonneplan provider: refresh-token OAuth2, then the consumer-prices chart.  Parses
//! quarter-hourly first and falls back to the legacy hourly chart; every slot's `market` is derived
//! once with the contract constants.  Credentials and tokens are never logged.

use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use crate::book::{PriceBook, PriceQuery};
use crate::error::PriceError;
use crate::provider::oauth::{AccessToken, app_headers, fetch_access_token};
use crate::provider::parse::{ProviderKind, ingest};
use crate::provider::parse_zonneplan::{CHART_HOUR, CHART_QUARTER, parse_chart};
use crate::provider::provider::{BoxFuture, PriceProvider};
use crate::provider::zonneplan_credentials::ZonneplanCredentials;
use crate::settlement::Contract;

/// Production app-API base URL.
pub const ZONNEPLAN_BASE_URL: &str = "https://app-api.zonneplan.nl";
const REQUEST_TIMEOUT_SECS: u64 = 20;

/// Zonneplan app-API provider.
pub struct ZonneplanDirect {
    client: reqwest::Client,
    credentials: ZonneplanCredentials,
    base_url: String,
    token: Mutex<Option<AccessToken>>,
}

impl ZonneplanDirect {
    /// Production provider for a refresh token.
    pub fn new(credentials: ZonneplanCredentials) -> Result<Self, PriceError> {
        Self::with_base_url(credentials, ZONNEPLAN_BASE_URL)
    }

    /// Provider against an explicit base URL (tests, staging).
    pub fn with_base_url(
        credentials: ZonneplanCredentials,
        base_url: impl Into<String>,
    ) -> Result<Self, PriceError> {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .user_agent("energy-rs")
            .build()
            .map_err(|err| PriceError::Http {
                url: base_url.clone(),
                reason: err.to_string(),
            })?;
        Ok(Self {
            client,
            credentials,
            base_url,
            token: Mutex::new(None),
        })
    }

    async fn access_token(&self) -> Result<String, PriceError> {
        let now = chrono::Utc::now().timestamp();
        if let Ok(guard) = self.token.lock()
            && let Some(cached) = guard.as_ref()
            && cached.expires_at > now
        {
            return Ok(cached.token.clone());
        }
        let fresh = fetch_access_token(&self.client, &self.base_url, &self.credentials).await?;
        let token = fresh.token.clone();
        if let Ok(mut guard) = self.token.lock() {
            *guard = Some(fresh);
        }
        Ok(token)
    }

    async fn fetch_book(
        &self,
        chart: &str,
        query: PriceQuery,
        contract: &Contract,
    ) -> Result<PriceBook, PriceError> {
        let token = self.access_token().await?;
        let payload = self.fetch_chart(&token, chart).await?;
        let series = parse_chart(&payload)?;
        ingest(series, ProviderKind::Zonneplan, contract, &query)
    }

    async fn fetch_chart(&self, token: &str, chart: &str) -> Result<Value, PriceError> {
        let url = format!("{}/api/consumer-prices/charts/{chart}", self.base_url);
        let response = app_headers(self.client.get(&url).bearer_auth(token))
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
                "Zonneplan chart {chart} refused with {status}"
            )));
        }
        if !status.is_success() {
            return Err(PriceError::Http {
                url,
                reason: format!("status {status}"),
            });
        }
        response.json::<Value>().await.map_err(|err| PriceError::Http {
            url,
            reason: format!("response is not JSON: {err}"),
        })
    }
}

impl PriceProvider for ZonneplanDirect {
    fn name(&self) -> &'static str {
        "zonneplan_direct"
    }

    fn fetch<'a>(
        &'a self,
        query: PriceQuery,
        contract: &'a Contract,
    ) -> BoxFuture<'a, Result<PriceBook, PriceError>> {
        Box::pin(async move {
            let book = match self.fetch_book(CHART_QUARTER, query, contract).await {
                Ok(book) => book,
                Err(PriceError::Auth(message)) => return Err(PriceError::Auth(message)),
                Err(primary) => {
                    tracing::warn!(
                        error = %primary,
                        "quarter-hourly prices unusable, retrying the hourly chart"
                    );
                    self.fetch_book(CHART_HOUR, query, contract).await?
                }
            };
            book.check_coverage(query.now(), 0)?;
            Ok(book)
        })
    }
}

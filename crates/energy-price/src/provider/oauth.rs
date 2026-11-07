//! Zonneplan OAuth2 refresh-token grant.  Only the token endpoint URL, the app headers and the
//! access token live here; the refresh token never leaves [`ZonneplanCredentials`] except in the
//! request body, and no error message ever contains a token.

use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::error::PriceError;
use crate::provider::zonneplan_credentials::ZonneplanCredentials;

/// App version the consumer-prices API expects (prices.md §1.1).
pub(crate) const APP_VERSION: &str = "5.10.1";
/// App environment header value.
pub(crate) const APP_ENVIRONMENT: &str = "production";

const TOKEN_TTL_SECS: i64 = 3600;
const TOKEN_REFRESH_MARGIN_SECS: i64 = 60;

/// A short-lived bearer token with its local expiry.
pub(crate) struct AccessToken {
    pub(crate) token: String,
    pub(crate) expires_at: i64,
}

/// Headers every Zonneplan app-API call needs.
pub(crate) fn app_headers(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    request
        .header("content-type", "application/json;charset=utf-8")
        .header("x-app-version", APP_VERSION)
        .header("x-app-environment", APP_ENVIRONMENT)
        .header("x-ha-integration", "energy-rs/0.1.0")
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Exchange the refresh token for an access token (`POST oauth/token`).
pub(crate) async fn fetch_access_token(
    client: &reqwest::Client,
    base_url: &str,
    credentials: &ZonneplanCredentials,
) -> Result<AccessToken, PriceError> {
    let url = format!("{base_url}/oauth/token");
    let body = json!({
        "grant_type": "refresh_token",
        "refresh_token": credentials.refresh_token(),
    });
    let response = app_headers(client.post(&url))
        .json(&body)
        .send()
        .await
        .map_err(|err| PriceError::Http {
            url: "oauth/token".to_owned(),
            reason: err.to_string(),
        })?;
    let status = response.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(PriceError::Auth(format!(
            "Zonneplan token endpoint refused with {status}"
        )));
    }
    if !status.is_success() {
        return Err(PriceError::Http {
            url: "oauth/token".to_owned(),
            reason: format!("status {status}"),
        });
    }
    let token: TokenResponse = response.json().await.map_err(|err| {
        PriceError::Payload(format!("Zonneplan token response is not JSON: {err}"))
    })?;
    if token.access_token.trim().is_empty() {
        return Err(PriceError::Auth(
            "Zonneplan returned an empty access token".to_owned(),
        ));
    }
    let ttl = token.expires_in.unwrap_or(TOKEN_TTL_SECS).clamp(0, 86_400);
    let now = Utc::now().timestamp();
    Ok(AccessToken {
        token: token.access_token,
        expires_at: now + ttl.saturating_sub(TOKEN_REFRESH_MARGIN_SECS),
    })
}

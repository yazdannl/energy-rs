//! REST client plus a supervised WebSocket connection.

use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::state::EntityState;

use super::{Config, Connection, CoreConfig, EventStream, HaError, Supervisor};

/// REST read path and a supervised WebSocket.
pub struct HaClient {
    config: Config,
    http: reqwest::Client,
    supervisor: Supervisor,
}

impl HaClient {
    pub fn new(config: Config) -> Result<Self, HaError> {
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .connect_timeout(config.connect_timeout)
            .user_agent(format!("energy-rs/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| HaError::Http(config.redact(&error.to_string())))?;
        let supervisor = Supervisor::spawn(config.clone());
        Ok(Self {
            config,
            http,
            supervisor,
        })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn supervisor(&self) -> &Supervisor {
        &self.supervisor
    }

    pub async fn connection(&self) -> Result<Arc<Connection>, HaError> {
        self.supervisor
            .connection_within(self.config.request_timeout)
            .await
    }

    pub async fn call<T: DeserializeOwned>(
        &self,
        command_type: &str,
        params: Value,
    ) -> Result<T, HaError> {
        self.connection().await?.call(command_type, params).await
    }

    pub async fn subscribe(
        &self,
        command_type: &str,
        params: Value,
    ) -> Result<EventStream, HaError> {
        self.connection()
            .await?
            .subscribe(command_type, params)
            .await
    }

    /// `GET /api/config` (timezone, currency, version; coordinates are not retained).
    pub async fn core_config(&self) -> Result<CoreConfig, HaError> {
        let value = self
            .rest_get("/config")
            .await?
            .ok_or_else(|| HaError::Protocol("GET /api/config returned 404".to_owned()))?;
        serde_json::from_value(value).map_err(HaError::from)
    }

    /// `GET /api/states/<entity_id>`; a 404 is `Ok(None)`.
    pub async fn state(&self, entity_id: &str) -> Result<Option<EntityState>, HaError> {
        let path = format!("/states/{}", entity_id.trim_start_matches('/'));
        match self.rest_get(&path).await? {
            Some(value) => serde_json::from_value(value)
                .map(Some)
                .map_err(HaError::from),
            None => Ok(None),
        }
    }

    /// `GET /api/states` — every state, unfiltered. Startup seeding only, never polling.
    pub async fn states(&self) -> Result<Vec<EntityState>, HaError> {
        let value = self
            .rest_get("/states")
            .await?
            .ok_or_else(|| HaError::Protocol("GET /api/states returned 404".to_owned()))?;
        serde_json::from_value(value).map_err(HaError::from)
    }

    async fn rest_get(&self, path: &str) -> Result<Option<Value>, HaError> {
        let url = format!(
            "{}/{}",
            self.config.rest_base().trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        let response = self
            .http
            .get(&url)
            .bearer_auth(self.config.token())
            .send()
            .await
            .map_err(|error| HaError::Http(self.config.redact(&error.to_string())))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(HaError::Auth(format!(
                "Home Assistant rejected the token (HTTP {})",
                status.as_u16()
            )));
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let body = self.config.redact(&truncate(&body, 400));
            return Err(HaError::Http(format!(
                "GET {} failed with HTTP {}: {}",
                path,
                status.as_u16(),
                body
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|error| HaError::Http(self.config.redact(&error.to_string())))?;
        serde_json::from_str(&body).map(Some).map_err(HaError::from)
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((index, _)) => format!("{}…", &text[..index]),
        None => text.to_owned(),
    }
}

//! Connection configuration and token redaction.

use std::{fmt, time::Duration};

use super::{HaError, ReconnectPolicy};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub(crate) struct Token(String);

impl Token {
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Connection settings.
///
/// Direct mode takes the HA root URL (`http://homeassistant:8123`) and a user token. Add-on mode
/// uses the Supervisor proxy (`http://supervisor/core`) with `SUPERVISOR_TOKEN`.
/// [`Config::from_env`] picks the right mode; the token is never logged and is redacted from errors.
#[derive(Clone)]
pub struct Config {
    rest_base: String,
    ws_url: String,
    pub(crate) token: Token,
    /// Timeout for one WebSocket command or REST request.
    pub request_timeout: Duration,
    /// Timeout for TCP/TLS connect plus the WS auth handshake.
    pub connect_timeout: Duration,
    /// Reconnect backoff policy.
    pub reconnect: ReconnectPolicy,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("rest_base", &self.rest_base)
            .field("ws_url", &self.ws_url)
            .field("token", &self.token)
            .field("request_timeout", &self.request_timeout)
            .field("connect_timeout", &self.connect_timeout)
            .field("reconnect", &self.reconnect)
            .finish()
    }
}

impl Config {
    /// Add-on mode: Supervisor proxy and token from the environment.
    pub fn supervisor(token: impl Into<String>) -> Self {
        Self {
            rest_base: "http://supervisor/core/api".to_owned(),
            ws_url: "ws://supervisor/core/websocket".to_owned(),
            token: Token(token.into()),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            reconnect: ReconnectPolicy::default(),
        }
    }

    /// Direct Core mode (development only); `base` is the HA root URL, not `/api`.
    pub fn direct(base: &str, token: impl Into<String>) -> Result<Self, HaError> {
        let token = token.into();
        if token.trim().is_empty() {
            return Err(HaError::Config("Home Assistant token is empty".to_owned()));
        }
        let url = reqwest::Url::parse(base)
            .map_err(|error| HaError::Config(format!("invalid Home Assistant URL: {error}")))?;
        let (http_scheme, ws_scheme) = match url.scheme() {
            "http" | "ws" => ("http", "ws"),
            "https" | "wss" => ("https", "wss"),
            other => {
                return Err(HaError::Config(format!(
                    "unsupported Home Assistant URL scheme {other:?}"
                )));
            }
        };
        if url.host_str().is_none() {
            return Err(HaError::Config("Home Assistant URL has no host".to_owned()));
        }

        let mut rest = url.clone();
        rest.set_scheme(http_scheme)
            .map_err(|()| HaError::Config("could not build REST URL".to_owned()))?;
        rest.set_path("/api");
        rest.set_query(None);
        rest.set_fragment(None);

        let mut ws = url;
        ws.set_scheme(ws_scheme)
            .map_err(|()| HaError::Config("could not build WebSocket URL".to_owned()))?;
        ws.set_path("/api/websocket");
        ws.set_query(None);
        ws.set_fragment(None);

        Ok(Self {
            rest_base: rest.to_string(),
            ws_url: ws.to_string(),
            token: Token(token),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            reconnect: ReconnectPolicy::default(),
        })
    }

    /// `SUPERVISOR_TOKEN`/`HASSIO_TOKEN` → add-on proxy; `HASS_URL` + `HASS_TOKEN` → direct.
    pub fn from_env() -> Result<Self, HaError> {
        if let Some(token) = env_value("SUPERVISOR_TOKEN") {
            return Ok(Self::supervisor(token));
        }
        if let Some(token) = env_value("HASSIO_TOKEN") {
            return Ok(Self::supervisor(token));
        }
        let url = env_value("HASS_URL")
            .or_else(|| env_value("HA_URL"))
            .ok_or_else(|| {
                HaError::Config(
                    "set SUPERVISOR_TOKEN (add-on), or HASS_URL and HASS_TOKEN (direct)".to_owned(),
                )
            })?;
        let token = env_value("HASS_TOKEN")
            .or_else(|| env_value("HA_TOKEN"))
            .ok_or_else(|| HaError::Config("HASS_TOKEN is not set".to_owned()))?;
        Self::direct(&url, token)
    }

    pub fn rest_base(&self) -> &str {
        &self.rest_base
    }

    pub fn ws_url(&self) -> &str {
        &self.ws_url
    }

    #[must_use]
    pub fn with_request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }

    #[must_use]
    pub fn with_connect_timeout(mut self, connect_timeout: Duration) -> Self {
        self.connect_timeout = connect_timeout;
        self
    }

    #[must_use]
    pub fn with_reconnect(mut self, reconnect: ReconnectPolicy) -> Self {
        self.reconnect = reconnect;
        self
    }

    /// Remove the token from text before it can reach a log or error.
    pub fn redact(&self, text: &str) -> String {
        let token = self.token.expose();
        if token.is_empty() {
            text.to_owned()
        } else {
            text.replace(token, "[REDACTED]")
        }
    }

    pub(crate) fn token(&self) -> &str {
        self.token.expose()
    }
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

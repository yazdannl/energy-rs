//! Typed Home Assistant errors. Messages never contain the access token.

use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum HaError {
    #[error("websocket: {0}")]
    Ws(String),
    #[error("websocket protocol: {0}")]
    Protocol(String),
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("Home Assistant error {code}: {message}")]
    Api { code: String, message: String },
    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("request timed out after {0:?}")]
    Timeout(Duration),
    #[error("Home Assistant connection is down")]
    Disconnected,
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("configuration: {0}")]
    Config(String),
}

impl From<serde_json::Error> for HaError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

impl HaError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Api { code, .. } if code == "not_found")
    }

    pub fn is_unknown_command(&self) -> bool {
        matches!(self, Self::Api { code, .. } if code == "unknown_command")
    }

    pub fn api_code(&self) -> Option<&str> {
        match self {
            Self::Api { code, .. } => Some(code),
            _ => None,
        }
    }
}

//! Versioned REST API.
//!
//! Contract for P0: `GET /api/v1/health` answers `{"status":"ok","version":"..."}` with 200 as long
//! as the process and HTTP server are alive. It must not depend on Home Assistant, the battery
//! fleet or the database, because the add-on watchdog polls it.

use axum::Json;
use axum::Router;
use axum::routing::get;
use serde::Serialize;

/// Health payload. Field names are part of the UI and watchdog contract.
#[derive(Debug, Clone, Serialize)]
pub struct Health {
    /// `"ok"` while the HTTP server is alive.
    pub status: &'static str,
    /// Binary version (`CARGO_PKG_VERSION`).
    pub version: &'static str,
}

/// `GET /api/v1/health`
pub async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// The `/api/v1` router (nested by [`crate::app::router`]).
pub fn router() -> Router {
    Router::new().route("/health", get(health))
}

#[cfg(test)]
mod tests {
    // A failed expectation is the test's report: `expect` is the intended API here.
    #![allow(clippy::expect_used)]

    use super::Health;

    #[test]
    fn health_payload_matches_the_contract() {
        let value = serde_json::to_value(Health {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
        })
        .expect("health payload serializes");

        assert_eq!(value["status"], "ok");
        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(value.as_object().map(serde_json::Map::len), Some(2));
    }
}

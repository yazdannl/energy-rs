//! `GET /api/v1/health` — liveness for the Supervisor watchdog.
//!
//! It must answer as long as the process and the HTTP server are alive, so it reads nothing but the
//! engine's last published snapshot (no Home Assistant, no battery, no database).

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::api::dto::Timestamp;
use crate::service::Service;

use energy_core::safety::Authority;

/// Health payload. `status` and `version` are the stable watchdog contract; the other two fields
/// are additive and may be ignored by a poller.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Health {
    status: &'static str,
    version: &'static str,
    mode: Authority,
    last_tick_at: Option<Timestamp>,
}

/// `GET /api/v1/health`
pub(crate) async fn health(State(service): State<Arc<Service>>) -> Json<Health> {
    let snapshot = service.snapshot();
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        mode: snapshot.mode,
        last_tick_at: Timestamp::lenient(snapshot.last_tick),
    })
}

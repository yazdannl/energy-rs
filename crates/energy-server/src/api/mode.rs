//! `PATCH /api/v1/mode` — choose `advisory`, `dry_run` or `auto`.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::blocking::with_ledger;
use crate::api::dto::Timestamp;
use crate::api::error::ApiError;
use crate::api::extract::ApiJson;
use crate::service::Service;

use energy_core::mode;
use energy_core::safety::Authority;
use energy_core::stream::EngineCommand;

/// The mode the engine is asked to obey.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModeRequest {
    mode: String,
}

/// Result of a mode change. `engineNotified` is always true on 200: the setting is persisted first,
/// then queued; a closed queue is a 503, never a silent success.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModeChange {
    mode: Authority,
    updated_at: Timestamp,
    engine_notified: bool,
}

fn parse_mode(value: &str) -> Result<Authority, ApiError> {
    Authority::parse(value.trim()).ok_or_else(|| {
        ApiError::validation("mode must be one of advisory, dry_run, auto").with_details(json!({
            "allowed": ["advisory", "dry_run", "auto"],
            "received": value,
        }))
    })
}

/// `PATCH /api/v1/mode`
pub(crate) async fn patch_mode(
    State(service): State<Arc<Service>>,
    ApiJson(body): ApiJson<ModeRequest>,
) -> Result<Json<ModeChange>, ApiError> {
    let mode = parse_mode(&body.mode)?;
    let now = Utc::now().timestamp();
    with_ledger(&service, move |ledger| {
        mode::save(ledger, mode, now).map_err(|err| ApiError::internal("write mode", err))
    })
    .await?;
    if let Err(refusal) = service.send_command(EngineCommand::SetMode(mode)) {
        return Err(ApiError::engine(refusal).with_details(json!({
            "reason": refusal.reason(),
            "persisted": true,
            "mode": mode,
        })));
    }
    Ok(Json(ModeChange {
        mode,
        updated_at: Timestamp::from_epoch(now)?,
        engine_notified: true,
    }))
}

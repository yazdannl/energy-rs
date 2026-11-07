//! `POST /api/v1/protection` — request the maintenance full charge now.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::error::ApiError;
use crate::api::extract::ApiJson;
use crate::api::pack_lookup::resolve_optional_pack;
use crate::service::Service;

use energy_core::stream::EngineCommand;

/// The only protection action the API exposes.
pub(crate) const FULL_CHARGE: &str = "full_charge";

/// The protection request.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProtectionRequest {
    action: String,
    pack: Option<String>,
}

/// What was queued.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Protection {
    accepted: bool,
    action: &'static str,
    pack: Option<String>,
}

/// `POST /api/v1/protection`
pub(crate) async fn protection(
    State(service): State<Arc<Service>>,
    ApiJson(body): ApiJson<ProtectionRequest>,
) -> Result<(StatusCode, Json<Protection>), ApiError> {
    if body.action.trim() != FULL_CHARGE {
        return Err(
            ApiError::validation("action must be full_charge").with_details(json!({
                "allowed": [FULL_CHARGE],
                "received": body.action,
            })),
        );
    }
    let pack = resolve_optional_pack(&service, body.pack.as_deref()).await?;
    service
        .send_command(EngineCommand::FullCharge { pack: pack.clone() })
        .map_err(ApiError::engine)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(Protection {
            accepted: true,
            action: FULL_CHARGE,
            pack,
        }),
    ))
}

//! `POST /api/v1/learning` — request a relearn pass.

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

/// The only learning action the API exposes.
pub(crate) const RELEARN: &str = "relearn";

/// The learning request.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LearningRequest {
    action: String,
    pack: Option<String>,
}

/// What was queued.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Learning {
    accepted: bool,
    action: &'static str,
    pack: Option<String>,
}

/// `POST /api/v1/learning`
pub(crate) async fn learning(
    State(service): State<Arc<Service>>,
    ApiJson(body): ApiJson<LearningRequest>,
) -> Result<(StatusCode, Json<Learning>), ApiError> {
    if body.action.trim() != RELEARN {
        return Err(
            ApiError::validation("action must be relearn").with_details(json!({
                "allowed": [RELEARN],
                "received": body.action,
            })),
        );
    }
    let pack = resolve_optional_pack(&service, body.pack.as_deref()).await?;
    service
        .send_command(EngineCommand::Relearn { pack: pack.clone() })
        .map_err(ApiError::engine)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(Learning {
            accepted: true,
            action: RELEARN,
            pack,
        }),
    ))
}

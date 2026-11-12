//! `POST /api/v1/replan` — solve a fresh plan now.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;

use crate::api::error::ApiError;
use crate::service::Service;

use energy_core::stream::EngineCommand;

/// What was queued.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Replan {
    accepted: bool,
}

/// `POST /api/v1/replan`
pub(crate) async fn replan(
    State(service): State<Arc<Service>>,
) -> Result<(StatusCode, Json<Replan>), ApiError> {
    service
        .send_command(EngineCommand::Replan)
        .map_err(ApiError::engine)?;
    Ok((StatusCode::ACCEPTED, Json(Replan { accepted: true })))
}

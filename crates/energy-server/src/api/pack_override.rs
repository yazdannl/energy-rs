//! `POST /api/v1/override` — pin one pack for a bounded time.
//!
//! The server validates everything the engine would otherwise have to guess (known pack, action,
//! power inside the pack's limit, TTL) and then queues the command. `202 Accepted` means the engine
//! has it, not that the pack has moved; the next snapshot shows the effect.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::dto::Timestamp;
use crate::api::dto::override_action::action_name;
use crate::api::dto::units::watts;
use crate::api::error::ApiError;
use crate::api::extract::ApiJson;
use crate::api::pack_lookup::resolve_pack;
use crate::service::Service;

use energy_core::dispatch::OverrideAction;
use energy_core::stream::{EngineCommand, OverrideCommand};

/// Default and maximum override duration, seconds.
const DEFAULT_TTL_S: u32 = 900;
const MAX_TTL_S: u32 = 86_400;
/// Charge power used when the request omits it and the pack has no known limit.
const DEFAULT_CHARGE_W: f64 = 800.0;

/// The override request.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverrideRequest {
    pack: String,
    action: String,
    power_w: Option<f64>,
    ttl_s: Option<u32>,
}

/// What was queued.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Override {
    accepted: bool,
    pack: String,
    action: &'static str,
    power_w: Option<f64>,
    ttl_s: u32,
    expires_at: Timestamp,
}

fn parse_action(value: &str) -> Result<OverrideAction, ApiError> {
    match value.trim() {
        "charge" => Ok(OverrideAction::Charge { watts: 0.0 }),
        "hold" => Ok(OverrideAction::Hold),
        "off" => Ok(OverrideAction::Off),
        other => Err(
            ApiError::validation("action must be one of charge, hold, off").with_details(json!({
                "allowed": ["charge", "hold", "off"],
                "received": other,
            })),
        ),
    }
}

fn charge_power(requested: Option<f64>, limit_w: Option<f64>) -> Result<f64, ApiError> {
    let ceiling = limit_w
        .filter(|value| *value > 0.0)
        .unwrap_or(DEFAULT_CHARGE_W);
    let power = requested.unwrap_or(ceiling);
    if !power.is_finite() || power <= 0.0 {
        return Err(ApiError::validation("powerW must be a positive number"));
    }
    if power > ceiling {
        return Err(
            ApiError::validation("powerW exceeds the pack's charge limit")
                .with_details(json!({ "maxPowerW": watts(ceiling) })),
        );
    }
    Ok(watts(power))
}

fn ttl(requested: Option<u32>) -> Result<u32, ApiError> {
    let ttl = requested.unwrap_or(DEFAULT_TTL_S);
    if ttl == 0 || ttl > MAX_TTL_S {
        return Err(ApiError::validation("ttlS must be between 1 and 86400")
            .with_details(json!({ "minTtlS": 1, "maxTtlS": MAX_TTL_S, "received": ttl })));
    }
    Ok(ttl)
}

/// `POST /api/v1/override`
pub(crate) async fn create_override(
    State(service): State<Arc<Service>>,
    ApiJson(body): ApiJson<OverrideRequest>,
) -> Result<(StatusCode, Json<Override>), ApiError> {
    let pack = resolve_pack(&service, &body.pack).await?;
    let ttl_s = ttl(body.ttl_s)?;
    let (action, power_w) = match parse_action(&body.action)? {
        OverrideAction::Charge { .. } => {
            let power = charge_power(body.power_w, pack.max_charge_w)?;
            (OverrideAction::Charge { watts: power }, Some(power))
        }
        other => (other, None),
    };
    service
        .send_command(EngineCommand::Override(OverrideCommand {
            pack: pack.sn.clone(),
            action,
            ttl_s,
        }))
        .map_err(ApiError::engine)?;
    let expires_at =
        Timestamp::from_epoch(Utc::now().timestamp().saturating_add(i64::from(ttl_s)))?;
    Ok((
        StatusCode::ACCEPTED,
        Json(Override {
            accepted: true,
            pack: pack.sn,
            action: action_name(action),
            power_w,
            ttl_s,
            expires_at,
        }),
    ))
}

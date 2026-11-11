//! `PATCH /api/v1/packs/{sn}` — the operator's alias for one pack.
//!
//! The alias is presentation only: nothing in the engine reads it. It has its own column so
//! discovery can keep writing `name` (Home Assistant's device name) without touching it, and it is
//! unique across the fleet because the command endpoints resolve a pack by alias as well
//! (`/override`, `/protection`, `/learning`). The path identifies the row by serial: an alias is
//! not a key.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::blocking::with_ledger;
use crate::api::error::ApiError;
use crate::api::extract::ApiJson;
use crate::service::Service;

use energy_ledger::{Ledger, Pack};

/// Longest alias the API accepts, characters.
const MAX_ALIAS_CHARS: usize = 40;

/// Keep `Some` for a present value, including `null`: serde's `Option` alone would fold `null`
/// into "the key is missing", and the two mean different things here.
fn present_value<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Value::deserialize(deserializer).map(Some)
}

/// The `PATCH` body. The key must be present — an empty object is a mistake, not a clear — and
/// `"alias": null` is how the label is removed.
#[derive(Debug, Deserialize)]
pub(crate) struct AliasRequest {
    #[serde(default, deserialize_with = "present_value")]
    alias: Option<Value>,
}

/// The pack identity after the write.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AliasResponse {
    sn: String,
    alias: Option<String>,
}

/// Validate the body and reduce it to the stored form (`None` clears, `Some` renames).
fn normalize(body: &AliasRequest) -> Result<Option<String>, ApiError> {
    let alias = match &body.alias {
        None => {
            return Err(ApiError::validation(
                "the request must contain `alias` (a string, or null to clear it)",
            ));
        }
        Some(Value::Null) => return Ok(None),
        Some(Value::String(text)) => text.trim(),
        Some(_) => return Err(ApiError::body("alias must be a string or null")),
    };
    let length = alias.chars().count();
    if length == 0 {
        return Ok(None);
    }
    if length > MAX_ALIAS_CHARS {
        return Err(ApiError::validation(format!(
            "alias must be at most {MAX_ALIAS_CHARS} characters"
        ))
        .with_details(json!({ "key": "alias", "max": MAX_ALIAS_CHARS, "received": length })));
    }
    if alias.chars().any(char::is_control) {
        return Err(
            ApiError::validation("alias may not contain control characters")
                .with_details(json!({ "key": "alias" })),
        );
    }
    Ok(Some(alias.to_owned()))
}

/// Write the alias against the ledger, refusing an alias another pack already holds.
fn store(ledger: &Ledger, sn: &str, alias: Option<&str>) -> Result<Pack, ApiError> {
    let packs = ledger
        .packs()
        .map_err(|err| ApiError::internal("write pack alias", err))?;
    if packs
        .by_sn(sn)
        .map_err(|err| ApiError::internal("look up pack", err))?
        .is_none()
    {
        return Err(ApiError::unknown_pack(sn));
    }
    if let Some(alias) = alias {
        let holder = packs
            .all()
            .map_err(|err| ApiError::internal("list packs", err))?
            .into_iter()
            .find(|pack| pack.sn != sn && pack.alias.as_deref() == Some(alias));
        if let Some(holder) = holder {
            return Err(
                ApiError::validation("alias is already used by another pack")
                    .with_details(json!({ "key": "alias", "sn": holder.sn })),
            );
        }
    }
    packs
        .set_alias(sn, alias)
        .map_err(|err| ApiError::internal("write pack alias", err))?
        .ok_or_else(|| ApiError::unknown_pack(sn))
}

/// `PATCH /api/v1/packs/{sn}`
pub(crate) async fn patch_alias(
    State(service): State<Arc<Service>>,
    Path(sn): Path<String>,
    ApiJson(body): ApiJson<AliasRequest>,
) -> Result<Json<AliasResponse>, ApiError> {
    let alias = normalize(&body)?;
    let pack = with_ledger(&service, move |ledger| store(ledger, &sn, alias.as_deref())).await?;
    Ok(Json(AliasResponse {
        sn: pack.sn,
        alias: pack.alias,
    }))
}

//! `PATCH /api/v1/packs/{sn}` — the operator's alias and pre-learning AC round trip.
//!
//! Both fields are presentation/control inputs, never learning inputs: the alias only labels the
//! pack and `roundTripPct` is stored on its own column, leaving the learner's profile, confidence
//! and timestamp untouched. The path identifies the row by serial; an alias is not a key.
//!
//! A finite percentage inside the physical envelope is stored as a fraction; `null` clears the
//! entry back to the typed default. A learned measurement still wins in every consumer.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::blocking::with_ledger;
use crate::api::error::ApiError;
use crate::api::extract::ApiJson;
use crate::service::Service;

use energy_battery::learning::round_trip::{from_percent, to_percent, valid_manual, valid_percent};
use energy_battery::learning::{MAX_ROUND_TRIP, MIN_ROUND_TRIP};
use energy_ledger::{Ledger, Pack, PackField};

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

/// The `PATCH` body. At least one key must be present; `"field": null` clears that field.
#[derive(Debug, Deserialize)]
pub(crate) struct PackPatchRequest {
    #[serde(default, deserialize_with = "present_value")]
    alias: Option<Value>,
    #[serde(default, rename = "roundTripPct", deserialize_with = "present_value")]
    round_trip_pct: Option<Value>,
}

/// One PATCH field: the key was absent, sent `null`, or sent a value.
enum Field<T> {
    Clear,
    Value(T),
}

impl<T> Field<T> {
    fn as_option(&self) -> Option<&T> {
        match self {
            Self::Clear => None,
            Self::Value(value) => Some(value),
        }
    }
}

/// Only the fields the operator actually sent.
struct Patch {
    alias: Option<Field<String>>,
    round_trip: Option<Field<f64>>,
}

/// The fields the operator just wrote.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackPatchResponse {
    sn: String,
    alias: Option<String>,
    manual_round_trip: Option<f64>,
    manual_round_trip_pct: Option<f64>,
}

/// Validate the alias, reducing it to the stored form (`None` clears, `Some` renames).
fn normalize_alias(value: &Value) -> Result<Option<String>, ApiError> {
    let alias = match value {
        Value::Null => return Ok(None),
        Value::String(text) => text.trim(),
        _ => return Err(ApiError::body("alias must be a string or null")),
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

/// One typed 422 for every unacceptable `roundTripPct`, naming the allowed range.
fn invalid_percent(received: &serde_json::Value) -> ApiError {
    let low = to_percent(MIN_ROUND_TRIP);
    let high = to_percent(MAX_ROUND_TRIP);
    ApiError::validation(format!(
        "roundTripPct must be between {low} and {high}, or null to clear the manual value"
    ))
    .with_details(json!({ "key": "roundTripPct", "min": low, "max": high, "received": received }))
}

/// Validate the percentage before any comparison: a non-finite number is never in range.
fn round_trip_from_percent(percent: f64) -> Result<f64, ApiError> {
    if !percent.is_finite() || !valid_percent(percent) {
        return Err(invalid_percent(&json!(percent)));
    }
    Ok(from_percent(percent))
}

/// Validate the body: a missing key means "leave alone", `null` means "clear".
fn normalize(body: &PackPatchRequest) -> Result<Patch, ApiError> {
    let alias = body
        .alias
        .as_ref()
        .map(|value| {
            normalize_alias(value).map(|value| match value {
                Some(value) => Field::Value(value),
                None => Field::Clear,
            })
        })
        .transpose()?;
    let round_trip = match &body.round_trip_pct {
        None => None,
        Some(Value::Null) => Some(Field::Clear),
        Some(Value::Number(number)) => match number.as_f64() {
            Some(percent) => Some(Field::Value(round_trip_from_percent(percent)?)),
            None => return Err(invalid_percent(&Value::Number(number.clone()))),
        },
        Some(value) => return Err(invalid_percent(value)),
    };
    if alias.is_none() && round_trip.is_none() {
        return Err(ApiError::validation(
            "the request must contain `alias` or `roundTripPct`",
        ));
    }
    Ok(Patch { alias, round_trip })
}

/// Validate and write both fields in one ledger transaction.
fn store(ledger: &Ledger, sn: &str, patch: &Patch) -> Result<Pack, ApiError> {
    let packs = ledger
        .packs()
        .map_err(|err| ApiError::internal("write pack fields", err))?;
    if packs
        .by_sn(sn)
        .map_err(|err| ApiError::internal("look up pack", err))?
        .is_none()
    {
        return Err(ApiError::unknown_pack(sn));
    }
    if let Some(alias) = patch.alias.as_ref().and_then(Field::as_option) {
        let holder = packs
            .all()
            .map_err(|err| ApiError::internal("list packs", err))?
            .into_iter()
            .find(|pack| pack.sn != sn && pack.alias.as_deref() == Some(alias.as_str()));
        if let Some(holder) = holder {
            return Err(
                ApiError::validation("alias is already used by another pack")
                    .with_details(json!({ "key": "alias", "sn": holder.sn })),
            );
        }
    }
    let alias = match &patch.alias {
        None => PackField::Keep,
        Some(Field::Clear) => PackField::Clear,
        Some(Field::Value(text)) => PackField::Set(text.as_str()),
    };
    let manual = match &patch.round_trip {
        None => PackField::Keep,
        Some(Field::Clear) => PackField::Clear,
        Some(Field::Value(value)) => PackField::Set(*value),
    };
    packs
        .patch(sn, alias, manual)
        .map_err(|err| ApiError::internal("write pack fields", err))?
        .ok_or_else(|| ApiError::unknown_pack(sn))
}

/// `PATCH /api/v1/packs/{sn}`
pub(crate) async fn patch_pack(
    State(service): State<Arc<Service>>,
    Path(sn): Path<String>,
    ApiJson(body): ApiJson<PackPatchRequest>,
) -> Result<Json<PackPatchResponse>, ApiError> {
    let patch = normalize(&body)?;
    let pack = with_ledger(&service, move |ledger| store(ledger, &sn, &patch)).await?;
    let manual_round_trip = pack.manual_round_trip.filter(|value| valid_manual(*value));
    Ok(Json(PackPatchResponse {
        sn: pack.sn,
        alias: pack.alias,
        manual_round_trip,
        manual_round_trip_pct: manual_round_trip.map(to_percent),
    }))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]
mod tests {
    use super::*;

    #[test]
    fn non_finite_percentages_are_rejected_by_the_validator() {
        for percent in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(round_trip_from_percent(percent).is_err());
        }
    }

    #[test]
    fn a_valid_percentage_becomes_its_fraction() {
        assert_eq!(round_trip_from_percent(81.0).expect("valid"), 0.81);
    }
}

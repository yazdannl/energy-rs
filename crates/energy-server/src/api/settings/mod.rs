//! `GET`/`PATCH /api/v1/settings` — writable, validated settings with as-of reads.
//!
//! Only the keys in [`schema`] are visible or writable. Values are stored in the ledger as text
//! (`settings` table) and rendered here typed. `PATCH` is timestamped: `at` defaults to now, and a
//! timestamp older than a stored change for a key is refused with `409 stale_write` instead of
//! rewriting history.

mod schema;
mod setting_kind;
mod setting_spec;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::api::blocking::with_ledger;
use crate::api::dto::{Timestamp, parse_rfc3339};
use crate::api::error::ApiError;
use crate::api::extract::{ApiJson, ApiQuery};
use crate::service::Service;

use schema::specs;
use setting_spec::SettingSpec;

/// How far into the future a timestamped write may point (clock skew).
const FUTURE_SKEW_S: i64 = 60;

/// Raw query strings.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsParams {
    as_of: Option<String>,
}

/// The `PATCH` body: a map of setting key to typed value plus the optional write instant.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PatchRequest {
    #[serde(default)]
    values: Map<String, Value>,
    at: Option<String>,
}

/// A writable setting and its type.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SchemaEntry {
    key: &'static str,
    kind: &'static str,
    default: Value,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    options: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max: Option<f64>,
}

/// `GET`/`PATCH /api/v1/settings` payload.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Settings {
    as_of: Timestamp,
    values: BTreeMap<String, Value>,
    schema: Vec<SchemaEntry>,
}

fn schema_entry(spec: &SettingSpec) -> SchemaEntry {
    let (kind, options, min, max) = spec.kind.describe();
    SchemaEntry {
        key: spec.key,
        kind,
        default: spec.kind.parse(spec.default).unwrap_or(Value::Null),
        options,
        min,
        max,
    }
}

fn render_values(stored: &BTreeMap<String, Option<String>>) -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    for spec in specs() {
        let raw = stored
            .get(spec.key)
            .and_then(|value| value.as_deref())
            .unwrap_or(spec.default);
        values.insert(
            spec.key.to_owned(),
            spec.kind.parse(raw).unwrap_or(Value::Null),
        );
    }
    values
}

fn read_values(
    ledger: &energy_ledger::Ledger,
    as_of: Option<i64>,
) -> Result<BTreeMap<String, Option<String>>, ApiError> {
    let settings = ledger
        .settings()
        .map_err(|err| ApiError::internal("read settings", err))?;
    let mut stored = BTreeMap::new();
    for spec in specs() {
        let value = match as_of {
            Some(at) => settings.as_of(spec.key, at),
            None => settings.get(spec.key),
        }
        .map_err(|err| ApiError::internal("read settings", err))?;
        stored.insert(spec.key.to_owned(), value);
    }
    Ok(stored)
}

/// `GET /api/v1/settings`
pub(crate) async fn get_settings(
    State(service): State<Arc<Service>>,
    ApiQuery(params): ApiQuery<SettingsParams>,
) -> Result<Json<Settings>, ApiError> {
    let as_of = match params.as_of.as_deref() {
        None => Timestamp::now(),
        Some(value) => Timestamp::from_datetime(parse_rfc3339(value)?),
    };
    let at = as_of.epoch();
    let stored = with_ledger(&service, move |ledger| read_values(ledger, Some(at))).await?;
    Ok(Json(Settings {
        as_of,
        values: render_values(&stored),
        schema: specs().iter().map(schema_entry).collect(),
    }))
}

/// Turn the JSON body value into the text form the ledger stores, validating it first.
fn value_to_stored(spec: &SettingSpec, value: &Value) -> Result<String, ApiError> {
    let stored = match value {
        Value::String(text) => text.trim().to_owned(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        other => {
            return Err(ApiError::invalid_setting(format!(
                "setting {:?} expects a string or number, not {other}",
                spec.key
            ))
            .with_details(json!({ "key": spec.key, "kind": spec.kind.describe().0 })));
        }
    };
    spec.kind.validate(&stored).map_err(|reason| {
        ApiError::invalid_setting(reason).with_details(json!({ "key": spec.key }))
    })?;
    Ok(stored)
}

fn validate_body(body: &PatchRequest) -> Result<Vec<(String, String)>, ApiError> {
    if body.values.is_empty() {
        return Err(ApiError::validation(
            "the request did not contain any settings",
        ));
    }
    let mut writes = Vec::new();
    for (key, value) in &body.values {
        if key == "mode" || key == energy_core::mode::MODE_KEY {
            return Err(ApiError::unsupported_setting(
                key,
                "use PATCH /api/v1/mode so the engine is notified",
            ));
        }
        let Some(spec) = schema::spec(key) else {
            return Err(ApiError::unsupported_setting(
                key,
                "not a known setting; see GET /api/v1/settings",
            ));
        };
        writes.push((key.clone(), value_to_stored(spec, value)?));
    }
    Ok(writes)
}

fn write_values(
    ledger: &energy_ledger::Ledger,
    writes: &[(String, String)],
    at: i64,
) -> Result<(), ApiError> {
    let settings = ledger
        .settings()
        .map_err(|err| ApiError::internal("write settings", err))?;
    for (key, _stored) in writes {
        let spec = schema::spec(key).ok_or_else(|| ApiError::internal("setting spec", key))?;
        let current = settings
            .as_of(spec.key, at)
            .map_err(|err| ApiError::internal("read settings", err))?;
        if current
            != settings
                .get(spec.key)
                .map_err(|err| ApiError::internal("read settings", err))?
        {
            return Err(ApiError::stale_write(spec.key, at));
        }
    }
    for (key, stored) in writes {
        settings
            .set(key, stored, at)
            .map_err(|err| ApiError::internal("write settings", err))?;
    }
    Ok(())
}

/// `PATCH /api/v1/settings`
pub(crate) async fn patch_settings(
    State(service): State<Arc<Service>>,
    ApiJson(body): ApiJson<PatchRequest>,
) -> Result<Json<Settings>, ApiError> {
    let now = chrono::Utc::now().timestamp();
    let at = match body.at.as_deref() {
        None => now,
        Some(value) => {
            let parsed = parse_rfc3339(value)?.timestamp();
            if parsed > now + FUTURE_SKEW_S {
                return Err(ApiError::validation("`at` may not lie in the future")
                    .with_details(json!({ "at": parsed, "now": now })));
            }
            parsed
        }
    };
    let writes = validate_body(&body)?;
    let keys = writes
        .iter()
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    let stored = with_ledger(&service, move |ledger| {
        write_values(ledger, &writes, at)?;
        read_values(ledger, None)
    })
    .await?;
    let mut values = render_values(&stored);
    values.retain(|key, _| keys.iter().any(|wanted| wanted == key));
    Ok(Json(Settings {
        as_of: Timestamp::from_epoch(at)?,
        values,
        schema: specs().iter().map(schema_entry).collect(),
    }))
}

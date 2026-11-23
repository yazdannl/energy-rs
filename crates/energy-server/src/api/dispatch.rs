//! `GET /api/v1/dispatch?limit` — the newest dispatch attempts, newest first.
//!
//! The window is the last seven days; the ledger keeps 20 000 rows, so a longer list than this is a
//! forensic question for the ledger, not a UI question.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::api::blocking::with_ledger;
use crate::api::dto::Timestamp;
use crate::api::dto::units::watts;
use crate::api::error::ApiError;
use crate::api::extract::ApiQuery;
use crate::api::params::limit;
use crate::service::Service;

use energy_ledger::DispatchEntry;

/// Window and list defaults.
const WINDOW_S: i64 = 7 * 24 * 3_600;
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 500;

/// Raw query strings.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DispatchParams {
    limit: Option<String>,
}

/// One dispatch attempt.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntryDto {
    at: Timestamp,
    pack_sn: Option<String>,
    requested_w: f64,
    /// `null` when no fresh measurement exists; never an assumed zero.
    applied_w: Option<f64>,
    /// Why the watts are unknown: `sample_stale` or `read_failed`.
    applied_w_reason: Option<String>,
    reason: String,
    mode: String,
    verified: bool,
    /// Whether a command reached the device this tick; absent on pre-v10 ledger rows.
    written: Option<bool>,
    /// Whether `applied_w` is a fresh measurement; false means the watts are unknown.
    flow_measured: bool,
    error: Option<String>,
}

/// `GET /api/v1/dispatch` payload.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Dispatch {
    window_seconds: i64,
    count: usize,
    entries: Vec<EntryDto>,
}

fn entry_dto(entry: &DispatchEntry, serials: &BTreeMap<i64, String>) -> Result<EntryDto, ApiError> {
    Ok(EntryDto {
        at: Timestamp::from_epoch(entry.ts)?,
        pack_sn: entry.pack_id.and_then(|id| serials.get(&id).cloned()),
        requested_w: watts(entry.requested_w),
        applied_w: entry.flow_measured.then(|| watts(entry.applied_w)),
        applied_w_reason: entry.applied_w_reason.clone(),
        reason: entry.reason.clone(),
        mode: entry.mode.clone(),
        verified: entry.verified,
        written: entry.written,
        flow_measured: entry.flow_measured,
        error: entry.error.clone(),
    })
}

/// `GET /api/v1/dispatch`
pub(crate) async fn dispatch(
    State(service): State<Arc<Service>>,
    ApiQuery(params): ApiQuery<DispatchParams>,
) -> Result<Json<Dispatch>, ApiError> {
    let max = limit(params.limit.as_deref(), DEFAULT_LIMIT, MAX_LIMIT)?;
    let now = Utc::now().timestamp();
    let from = now.saturating_sub(WINDOW_S);
    let (rows, serials) = with_ledger(&service, move |ledger| {
        let rows = ledger
            .dispatch()
            .map_err(|err| ApiError::internal("list dispatch", err))?
            .range(from, now.saturating_add(1))
            .map_err(|err| ApiError::internal("list dispatch", err))?;
        let packs = ledger
            .packs()
            .map_err(|err| ApiError::internal("list packs", err))?
            .all()
            .map_err(|err| ApiError::internal("list packs", err))?;
        let serials = packs
            .into_iter()
            .map(|pack| (pack.id, pack.sn))
            .collect::<BTreeMap<_, _>>();
        Ok((rows, serials))
    })
    .await?;
    let entries = rows
        .iter()
        .rev()
        .take(max)
        .map(|entry| entry_dto(entry, &serials))
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Json(Dispatch {
        window_seconds: WINDOW_S,
        count: entries.len(),
        entries,
    }))
}

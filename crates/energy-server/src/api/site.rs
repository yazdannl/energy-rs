//! `GET /api/v1/site` — what was discovered and what the engine can do.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::api::blocking::with_ledger;
use crate::api::dto::Timestamp;
use crate::api::error::ApiError;
use crate::service::Service;

use energy_core::safety::Authority;
use energy_ledger::Pack;

/// One discovered energy source.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceDto {
    kind: String,
    entity_id: String,
    label: Option<String>,
}

/// One known pack, identity only (live state lives in `/packs`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackDto {
    sn: String,
    name: Option<String>,
    label_capacity_kwh: f64,
    reported_capacity_kwh: Option<f64>,
    max_charge_w: Option<f64>,
    max_discharge_w: Option<f64>,
    first_seen_at: Timestamp,
    last_seen_at: Timestamp,
}

/// `GET /api/v1/site` payload.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Site {
    configured: bool,
    sources: Vec<SourceDto>,
    packs: Vec<PackDto>,
    capabilities: Vec<String>,
    mode: Authority,
    last_tick_at: Option<Timestamp>,
}

fn pack_dto(pack: &Pack) -> Result<PackDto, ApiError> {
    Ok(PackDto {
        sn: pack.sn.clone(),
        name: pack.name.clone(),
        label_capacity_kwh: pack.label_capacity_kwh,
        reported_capacity_kwh: pack.reported_capacity_kwh,
        max_charge_w: pack.max_charge_w,
        max_discharge_w: pack.max_discharge_w,
        first_seen_at: Timestamp::from_epoch(pack.first_seen)?,
        last_seen_at: Timestamp::from_epoch(pack.last_seen)?,
    })
}

/// `GET /api/v1/site`
pub(crate) async fn site(State(service): State<Arc<Service>>) -> Result<Json<Site>, ApiError> {
    let packs = with_ledger(&service, |ledger| {
        ledger
            .packs()
            .map_err(|err| ApiError::internal("list packs", err))?
            .all()
            .map_err(|err| ApiError::internal("list packs", err))
    })
    .await?;
    let packs = packs
        .iter()
        .map(pack_dto)
        .collect::<Result<Vec<_>, ApiError>>()?;

    let snapshot = service.snapshot();
    let sources = snapshot
        .site
        .sources
        .iter()
        .map(|source| SourceDto {
            kind: source.kind.clone(),
            entity_id: source.entity_id.clone(),
            label: source.label.clone(),
        })
        .collect();
    Ok(Json(Site {
        configured: snapshot.site.configured,
        sources,
        packs,
        capabilities: snapshot.site.capabilities.clone(),
        mode: snapshot.mode,
        last_tick_at: Timestamp::lenient(snapshot.last_tick),
    }))
}

//! `GET /api/v1/packs` — state, learned numbers, protection latch and limits per pack.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::api::blocking::with_ledger;
use crate::api::dto::Timestamp;
use crate::api::dto::units::watts;
use crate::api::error::ApiError;
use crate::api::state_dto::{StateDto, state_dto};
use crate::service::Service;

use energy_battery::learning::PairDiagnostics;
use energy_battery::{CellLatch, LearningProfile};
use energy_core::dispatch::{ManualOverride, OverrideAction};
use energy_core::stream::PackSnapshot;
use energy_ledger::PackState;

/// The applied learning result, including its provenance.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LearnedDto {
    capacity_kwh: Option<f64>,
    capacity_confidence: f64,
    capacity_applied: bool,
    capacity_decision: &'static str,
    capacity_band_low_kwh: f64,
    capacity_band_high_kwh: f64,
    /// `label`, `pooled`, `own` or `previous`: where the published capacity came from.
    capacity_source: &'static str,
    /// SoC points covered by the surviving cycles.
    soc_points_covered: f64,
    /// Usable cycles behind the capacity estimate.
    cycles_used: usize,
    round_trip: f64,
    eta_charge: f64,
    eta_discharge: f64,
    efficiency_confidence: f64,
    efficiency_applied: bool,
    /// `measured` when closed cycles produced the number, `default` while it is the typed fallback.
    round_trip_source: &'static str,
    /// Session counts, pair refusals and the round trip each candidate implied.
    efficiency_diagnostics: PairDiagnostics,
    sessions_used: usize,
    updated_at: Timestamp,
}

/// The low-cell protection latch.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LatchDto {
    blocked: bool,
    force_charge: bool,
    force_charge_w: Option<u32>,
    since: Timestamp,
    reason: &'static str,
    reason_at: Timestamp,
    voltage_v: Option<f64>,
    last_fresh_at: Option<Timestamp>,
}

/// Per-pack balancing history and the interval derived from it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MaintenanceDto {
    last_full_charge: Option<Timestamp>,
    next_due: Option<Timestamp>,
    dwell_state: &'static str,
    dwell_started_at: Option<Timestamp>,
    target_soc_pct: Option<f64>,
    unassigned_reason: Option<&'static str>,
    missing_kwh: Option<f64>,
}

/// A manual override that is still in force.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverrideDto {
    action: &'static str,
    power_w: Option<f64>,
    expires_at: Timestamp,
}

/// One pack row.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackDto {
    sn: String,
    name: Option<String>,
    alias: Option<String>,
    label_capacity_kwh: f64,
    reported_capacity_kwh: Option<f64>,
    limits: LimitsDto,
    first_seen_at: Timestamp,
    last_seen_at: Timestamp,
    state: StateDto,
    learned: Option<LearnedDto>,
    latch: Option<LatchDto>,
    #[serde(rename = "override")]
    override_state: Option<OverrideDto>,
    maintenance: MaintenanceDto,
}

/// Effective limits after config and device clamping.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LimitsDto {
    max_charge_w: Option<f64>,
    max_discharge_w: Option<f64>,
}

/// `GET /api/v1/packs` payload.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Packs {
    packs: Vec<PackDto>,
}

fn learned_dto(profile: &LearningProfile) -> LearnedDto {
    LearnedDto {
        capacity_kwh: profile.capacity_kwh,
        capacity_confidence: profile.capacity_confidence,
        capacity_applied: profile.capacity_applied,
        capacity_decision: profile.capacity_decision.slug(),
        capacity_band_low_kwh: profile.band_low_kwh,
        capacity_band_high_kwh: profile.band_high_kwh,
        capacity_source: profile.capacity_source.slug(),
        soc_points_covered: profile.soc_points_covered,
        cycles_used: profile.cycles_used,
        round_trip: profile.round_trip,
        eta_charge: profile.eta_charge,
        eta_discharge: profile.eta_discharge,
        efficiency_confidence: profile.efficiency_confidence,
        efficiency_applied: profile.efficiency_applied,
        round_trip_source: if profile.efficiency_applied {
            "measured"
        } else {
            "default"
        },
        efficiency_diagnostics: profile.efficiency_diagnostics.clone(),
        sessions_used: profile.sessions_used,
        updated_at: Timestamp::from_datetime(profile.updated_at),
    }
}

fn latch_dto(latch: &CellLatch) -> LatchDto {
    LatchDto {
        blocked: latch.blocked,
        force_charge: latch.force_charge,
        force_charge_w: latch.force_charge_w,
        since: Timestamp::from_datetime(latch.since),
        reason: latch.reason.slug(),
        reason_at: Timestamp::from_datetime(latch.reason_at),
        voltage_v: latch.voltage_v,
        last_fresh_at: latch.last_fresh_at.map(Timestamp::from_datetime),
    }
}

fn override_dto(state: &ManualOverride) -> Result<OverrideDto, ApiError> {
    let power_w = match state.action {
        OverrideAction::Charge { watts: request } => Some(watts(request)),
        OverrideAction::Discharge { watts: request } => Some(watts(-request)),
        OverrideAction::Hold | OverrideAction::Off => None,
    };
    Ok(OverrideDto {
        action: crate::api::dto::override_action::action_name(state.action),
        power_w,
        expires_at: Timestamp::from_epoch(state.until)?,
    })
}

fn maintenance_dto(
    stored: &PackState,
    live: Option<&PackSnapshot>,
    interval_days: i64,
) -> MaintenanceDto {
    let pack = &stored.pack;
    let last = pack.last_full_charge_at;
    let live_state = live.and_then(|snapshot| snapshot.maintenance.as_ref());
    let dwell_state = if let Some(live) = live_state {
        live.state
    } else if pack.dwell_started_at.is_some() {
        "dwell"
    } else if last.is_none() {
        "due"
    } else {
        "idle"
    };
    MaintenanceDto {
        last_full_charge: Timestamp::lenient(last),
        next_due: Timestamp::lenient(last.map(|at| at.saturating_add(interval_days * 86_400))),
        dwell_state,
        dwell_started_at: Timestamp::lenient(pack.dwell_started_at),
        target_soc_pct: live_state.map(|entry| entry.target_soc_pct),
        unassigned_reason: live_state.and_then(|entry| entry.unassigned_reason),
        missing_kwh: live_state.and_then(|entry| entry.missing_kwh),
    }
}

fn pack_dto(
    stored: &PackState,
    live: Option<&PackSnapshot>,
    interval_days: i64,
) -> Result<PackDto, ApiError> {
    let pack = &stored.pack;
    Ok(PackDto {
        sn: pack.sn.clone(),
        name: pack.name.clone(),
        alias: pack.alias.clone(),
        label_capacity_kwh: pack.label_capacity_kwh,
        reported_capacity_kwh: pack.reported_capacity_kwh,
        limits: LimitsDto {
            max_charge_w: live
                .and_then(|snapshot| snapshot.max_charge_w)
                .or(pack.max_charge_w),
            max_discharge_w: live
                .and_then(|snapshot| snapshot.max_discharge_w)
                .or(pack.max_discharge_w),
        },
        first_seen_at: Timestamp::from_epoch(pack.first_seen)?,
        last_seen_at: Timestamp::from_epoch(pack.last_seen)?,
        state: state_dto(live, stored),
        learned: live
            .and_then(|snapshot| snapshot.profile.as_ref())
            .map(learned_dto),
        latch: live
            .and_then(|snapshot| snapshot.latch.as_ref())
            .map(latch_dto),
        override_state: live
            .and_then(|snapshot| snapshot.override_state.as_ref())
            .map(override_dto)
            .transpose()?,
        maintenance: maintenance_dto(stored, live, interval_days),
    })
}

/// `GET /api/v1/packs`
pub(crate) async fn packs(State(service): State<Arc<Service>>) -> Result<Json<Packs>, ApiError> {
    let (stored, interval_days) = with_ledger(&service, |ledger| {
        let states = ledger
            .packs()
            .map_err(|err| ApiError::internal("list pack states", err))?
            .states()
            .map_err(|err| ApiError::internal("list pack states", err))?;
        let interval = ledger
            .settings()
            .ok()
            .and_then(|settings| settings.get("full_charge_interval_days").ok().flatten())
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(7)
            .clamp(1, 60);
        Ok((states, interval))
    })
    .await?;
    let snapshot = service.snapshot();
    let packs = stored
        .iter()
        .map(|state| {
            let live = snapshot
                .packs
                .iter()
                .find(|candidate| candidate.sn == state.pack.sn);
            pack_dto(state, live, interval_days)
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Json(Packs { packs }))
}

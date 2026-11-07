//! `GET /api/v1/plan/current` — the plan the engine is following, with its timeline.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::api::blocking::with_ledger;
use crate::api::dto::units::{kw_slot_to_wh, kw_to_w, kwh_to_wh};
use crate::api::dto::{Euro, Timestamp};
use crate::api::error::ApiError;
use crate::service::Service;

use energy_core::plan::{PlanSummary, SLOT_SECONDS};
use energy_ledger::PlanSlot;

/// Money and energy totals of the planned path.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SummaryDto {
    cost_eur: Euro,
    baseline_cost_eur: Euro,
    saved_eur: Euro,
    charged_wh: f64,
    discharged_wh: f64,
    grid_charge_wh: f64,
    pv_charge_wh: f64,
    cycles: f64,
    terminal_target_wh: Option<f64>,
    infeasible_reason: Option<String>,
}

/// One 15-minute slot. Power is W (positive charges), energy is Wh over the slot.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SlotDto {
    idx: i64,
    start_at: Timestamp,
    end_at: Timestamp,
    buy_eur_per_kwh: Euro,
    feed_in_eur_per_kwh: Euro,
    pv_wh: f64,
    load_wh: f64,
    battery_power_w: f64,
    soc_wh: f64,
    target_soc_wh: f64,
    reason: String,
}

/// `GET /api/v1/plan/current` payload.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CurrentPlan {
    plan_id: i64,
    created_at: Timestamp,
    mode: String,
    horizon_start_at: Timestamp,
    horizon_end_at: Timestamp,
    feasible: bool,
    summary: SummaryDto,
    binding_reasons: Vec<String>,
    slots: Vec<SlotDto>,
}

fn summary_dto(summary: &PlanSummary) -> SummaryDto {
    SummaryDto {
        cost_eur: Euro::new(summary.cost_estimate_eur),
        baseline_cost_eur: Euro::new(summary.baseline_cost_eur),
        saved_eur: Euro::new(summary.saved_eur),
        charged_wh: kwh_to_wh(summary.charged_kwh),
        discharged_wh: kwh_to_wh(summary.discharged_kwh),
        grid_charge_wh: kwh_to_wh(summary.grid_charge_kwh),
        pv_charge_wh: kwh_to_wh(summary.pv_charge_kwh),
        cycles: summary.cycles,
        terminal_target_wh: summary.terminal_target_kwh.map(kwh_to_wh),
        infeasible_reason: summary.infeasible_reason.clone(),
    }
}

fn slot_dto(slot: &PlanSlot) -> Result<SlotDto, ApiError> {
    let start = slot.t_start;
    Ok(SlotDto {
        idx: slot.idx,
        start_at: Timestamp::from_epoch(start)?,
        end_at: Timestamp::from_epoch(start.saturating_add(SLOT_SECONDS))?,
        buy_eur_per_kwh: Euro::new(slot.buy_all_in),
        feed_in_eur_per_kwh: Euro::new(slot.feed_in),
        pv_wh: kw_slot_to_wh(slot.pv_kw, SLOT_SECONDS),
        load_wh: kw_slot_to_wh(slot.load_kw, SLOT_SECONDS),
        battery_power_w: kw_to_w(slot.battery_kw),
        soc_wh: kwh_to_wh(slot.soc_kwh),
        target_soc_wh: kwh_to_wh(slot.target_soc_kwh),
        reason: slot.reason.clone(),
    })
}

/// The reasons that shaped the plan: the engine's own when it published this plan id, otherwise the
/// per-slot rationale (deduplicated, empty reasons dropped).
fn binding_reasons(
    service: &Service,
    plan_id: i64,
    slots: &[PlanSlot],
    infeasible: bool,
) -> Vec<String> {
    let snapshot = service.snapshot();
    let mut reasons = if let Some(plan) = snapshot.plan.filter(|plan| plan.plan_id == plan_id) {
        plan.binding_reasons
    } else {
        let mut unique: Vec<String> = Vec::new();
        for slot in slots {
            let reason = slot.reason.trim();
            if !reason.is_empty() && !unique.iter().any(|seen| seen == reason) {
                unique.push(reason.to_owned());
            }
        }
        unique
    };
    if infeasible
        && !reasons
            .iter()
            .any(|reason| reason == "terminal_target_unreachable")
    {
        reasons.push("terminal_target_unreachable".to_owned());
    }
    reasons
}

/// `GET /api/v1/plan/current`
pub(crate) async fn current_plan(
    State(service): State<Arc<Service>>,
) -> Result<Json<CurrentPlan>, ApiError> {
    let stored = with_ledger(&service, |ledger| {
        let plans = ledger
            .plans()
            .map_err(|err| ApiError::internal("current plan", err))?;
        let plan = plans
            .current()
            .map_err(|err| ApiError::internal("current plan", err))?;
        match plan {
            None => Ok(None),
            Some(plan) => {
                let slots = plans
                    .slots(plan.id)
                    .map_err(|err| ApiError::internal("plan slots", err))?;
                Ok(Some((plan, slots)))
            }
        }
    })
    .await?
    .ok_or_else(|| ApiError::not_found("there is no current plan"))?;

    let (plan, slots) = stored;
    let summary: PlanSummary =
        serde_json::from_str(&plan.summary_json).map_err(ApiError::corrupt_plan)?;
    let feasible = summary.infeasible_reason.is_none();
    let reasons = binding_reasons(&service, plan.id, &slots, !feasible);
    let slots = slots
        .iter()
        .map(slot_dto)
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Json(CurrentPlan {
        plan_id: plan.id,
        created_at: Timestamp::from_epoch(plan.created_at)?,
        mode: plan.mode,
        horizon_start_at: Timestamp::from_epoch(plan.horizon_start)?,
        horizon_end_at: Timestamp::from_epoch(plan.horizon_end)?,
        feasible,
        summary: summary_dto(&summary),
        binding_reasons: reasons,
        slots,
    }))
}

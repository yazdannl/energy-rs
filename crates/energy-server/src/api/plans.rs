//! `GET /api/v1/plans?limit` — the newest stored plans, newest first.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::api::blocking::with_ledger;
use crate::api::dto::{Euro, Timestamp};
use crate::api::error::ApiError;
use crate::api::extract::ApiQuery;
use crate::api::params::limit;
use crate::service::Service;

use energy_core::plan::PlanSummary;
use energy_ledger::Plan;

/// The default and maximum number of plans one request returns.
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

/// Raw query strings.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlansParams {
    limit: Option<String>,
}

/// One plan header. `costEur`/`savedEur`/`feasible` are `None` when the stored summary is corrupt.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanRow {
    plan_id: i64,
    created_at: Timestamp,
    mode: String,
    horizon_start_at: Timestamp,
    horizon_end_at: Timestamp,
    cost_eur: Option<Euro>,
    baseline_cost_eur: Option<Euro>,
    saved_eur: Option<Euro>,
    feasible: Option<bool>,
    infeasible_reason: Option<String>,
}

/// `GET /api/v1/plans` payload.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Plans {
    count: usize,
    plans: Vec<PlanRow>,
}

fn plan_row(plan: &Plan) -> Result<PlanRow, ApiError> {
    let summary: Option<PlanSummary> = serde_json::from_str(&plan.summary_json).ok();
    Ok(PlanRow {
        plan_id: plan.id,
        created_at: Timestamp::from_epoch(plan.created_at)?,
        mode: plan.mode.clone(),
        horizon_start_at: Timestamp::from_epoch(plan.horizon_start)?,
        horizon_end_at: Timestamp::from_epoch(plan.horizon_end)?,
        cost_eur: summary
            .as_ref()
            .map(|value| Euro::new(value.cost_estimate_eur)),
        baseline_cost_eur: summary
            .as_ref()
            .map(|value| Euro::new(value.baseline_cost_eur)),
        saved_eur: summary.as_ref().map(|value| Euro::new(value.saved_eur)),
        feasible: summary
            .as_ref()
            .map(|value| value.infeasible_reason.is_none()),
        infeasible_reason: summary.and_then(|value| value.infeasible_reason),
    })
}

/// `GET /api/v1/plans`
pub(crate) async fn plans(
    State(service): State<Arc<Service>>,
    ApiQuery(params): ApiQuery<PlansParams>,
) -> Result<Json<Plans>, ApiError> {
    let max = limit(params.limit.as_deref(), DEFAULT_LIMIT, MAX_LIMIT)?;
    let now = Utc::now().timestamp();
    let rows = with_ledger(&service, move |ledger| {
        ledger
            .plans()
            .map_err(|err| ApiError::internal("list plans", err))?
            .range(0, now.saturating_add(1))
            .map_err(|err| ApiError::internal("list plans", err))
    })
    .await?;
    let rows = rows
        .iter()
        .rev()
        .take(max)
        .map(plan_row)
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Json(Plans {
        count: rows.len(),
        plans: rows,
    }))
}

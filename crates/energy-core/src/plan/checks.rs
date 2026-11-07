//! The builder's refusals: bad packs, short/non-finite/stale forecasts and bad targets.

use crate::plan::{HORIZON_SLOTS, PlanError, PlanInput};

pub(crate) fn validate(input: &PlanInput) -> Result<(), PlanError> {
    check_horizon(input)?;
    check_packs(input)?;
    check_forecasts(input)?;
    check_target(input)
}

fn check_horizon(input: &PlanInput) -> Result<(), PlanError> {
    if input.horizon_slots == 0 || input.horizon_slots > HORIZON_SLOTS {
        return Err(PlanError::HorizonSlots {
            slots: input.horizon_slots,
        });
    }
    Ok(())
}

fn check_packs(input: &PlanInput) -> Result<(), PlanError> {
    if input.packs.is_empty() {
        return Err(PlanError::NoPacks);
    }
    for state in &input.packs {
        if let Some(issue) = state.spec.invalid() {
            return Err(PlanError::InvalidPack {
                pack_id: state.spec.pack_id.clone(),
                issue,
            });
        }
        if !state.soc_pct.is_finite() || state.soc_pct < 0.0 || state.soc_pct > 100.0 {
            return Err(PlanError::MissingSoc {
                pack_id: state.spec.pack_id.clone(),
            });
        }
    }
    Ok(())
}

fn check_forecasts(input: &PlanInput) -> Result<(), PlanError> {
    let limit = input
        .replan_seconds
        .saturating_mul(2)
        .max(input.stale_plan_seconds);
    let as_of = input
        .forecast_as_of
        .ok_or(PlanError::ForecastMissing { name: "pv/load" })?;
    let age = input.now.saturating_sub(as_of);
    if age > limit {
        return Err(PlanError::StaleForecast {
            name: "pv/load",
            age_s: age,
            limit_s: limit,
        });
    }
    check_series("pv_kw", &input.pv_kw, input.horizon_slots)?;
    check_series("load_kw", &input.load_kw, input.horizon_slots)
}

fn check_series(name: &'static str, series: &[f64], expected: usize) -> Result<(), PlanError> {
    if series.len() != expected {
        return Err(PlanError::ForecastLength {
            name,
            expected,
            got: series.len(),
        });
    }
    for (index, value) in series.iter().enumerate() {
        if !value.is_finite() {
            return Err(PlanError::ForecastNotFinite { name, index });
        }
    }
    Ok(())
}

fn check_target(input: &PlanInput) -> Result<(), PlanError> {
    if let Some(pct) = input.terminal_target_pct
        && (!pct.is_finite() || !(0.0..=100.0).contains(&pct))
    {
        return Err(PlanError::TerminalTargetPct { pct });
    }
    Ok(())
}

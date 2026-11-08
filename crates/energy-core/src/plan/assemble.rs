//! Orchestrate one plan: validate, solve, assemble, summarize.

use energy_opt::SolveInput;
use energy_price::{ExportSource, Slot};

use crate::plan::aggregate;
use crate::plan::config;
use crate::plan::differential::Differential;
use crate::plan::summary;
use crate::plan::timeline;
use crate::plan::{Plan, PlanError, PlanInput, SLOT_MINUTES, SLOT_SECONDS, checks, horizon};

pub(crate) fn build(input: &PlanInput, check: Option<&Differential>) -> Result<Plan, PlanError> {
    checks::validate(input)?;
    let slots = horizon::slots(input)?;
    let aggregate = aggregate::aggregate(input);
    let floor = sell_floor(input, &slots);
    let config = config::solver_config(input, &aggregate, floor);
    let solved = solve(input, &slots, &config, check)?;
    let rows = timeline::timeline(
        input,
        &slots,
        &solved,
        (aggregate.eta_charge, aggregate.eta_discharge),
    )?;
    let mut summary = summary::summarize(&rows, aggregate.capacity_kwh, config.e_target);
    if config.e_target.is_some() && solved.terminal_error_kwh > 0.05 {
        summary.infeasible_reason = Some("terminal_target_unreachable".to_owned());
    }
    Ok(Plan {
        generated_at: input.now,
        horizon_start: slots.first().map_or(input.now, |slot| slot.t_start),
        horizon_end: slots
            .last()
            .map_or(input.now, |slot| slot.t_start + SLOT_SECONDS),
        feasible: summary.infeasible_reason.is_none(),
        objective_eur: solved.objective_eur,
        sell_floor: Some(floor),
        summary,
        degraded: input.forecast_note.clone(),
        slots: rows,
    })
}

fn solve(
    input: &PlanInput,
    slots: &[Slot],
    config: &energy_opt::Config,
    check: Option<&Differential>,
) -> Result<energy_opt::SolveResult, PlanError> {
    if slots.len() != input.horizon_slots {
        return Err(PlanError::SolveShape {
            expected: input.horizon_slots,
            got: slots.len(),
        });
    }
    let buy: Vec<f64> = slots.iter().map(|slot| slot.buy_all_in).collect();
    let sell: Vec<f64> = slots
        .iter()
        .map(|slot| timeline::feed_in(&input.settlement, slot))
        .collect();
    let minutes = vec![SLOT_MINUTES; slots.len()];
    let scenario = SolveInput::new(&input.pv_kw, &input.load_kw, &buy, &sell, &minutes);
    let solved = energy_opt::solve(&scenario, config)?;
    // The slow path is allowed here and nowhere else: one solve per interval is re-derived by the
    // oracle, and a disagreement refuses the plan instead of shipping it (PLAN.md §6).
    if let Some(check) = check {
        check.verify(&scenario, config, &solved)?;
    }
    Ok(solved)
}

/// The strictest marginal export value over the horizon; the plan's own sell floor.
fn sell_floor(input: &PlanInput, slots: &[Slot]) -> f64 {
    slots
        .iter()
        .map(|slot| {
            input
                .settlement
                .marginal_export(slot, ExportSource::Battery)
        })
        .fold(f64::INFINITY, f64::min)
}

//! Orchestrate one plan: validate, solve, assemble, summarize.

use energy_opt::SolveInput;
use energy_price::{ExportSource, Slot};

use crate::plan::aggregate::{self, Aggregate};
use crate::plan::clamp::{self, Request};
use crate::plan::config;
use crate::plan::differential::Differential;
use crate::plan::summary;
use crate::plan::timeline;
use crate::plan::{
    ExportPolicy, Plan, PlanError, PlanInput, Policy, SLOT_MINUTES, SLOT_SECONDS, checks, horizon,
};

pub(crate) fn build(input: &PlanInput, check: Option<&Differential>) -> Result<Plan, PlanError> {
    checks::validate(input)?;
    let slots = horizon::slots(input)?;
    let aggregate = aggregate::aggregate(input);
    let basis = resolve_basis(input, &slots, &aggregate)?;
    let policy = Policy {
        cost_basis_eur_per_kwh: basis,
        ..input.policy
    };
    let config = config::solver_config(input, &aggregate, salvage(input, &slots, &aggregate));
    let solved = solve(input, &slots, &config, check)?;
    let outcome = clamp_solved(input, &aggregate, policy, &solved);
    let rows = timeline::timeline(
        input,
        &slots,
        &outcome.energies,
        &outcome.powers,
        (aggregate.eta_charge, aggregate.eta_discharge),
    )?;
    let rows = with_clamp_reasons(rows, &outcome.flags);
    let mut summary = summary::summarize(&rows, aggregate.capacity_kwh, config.e_target);
    let sale_floor = sale_floor(&policy);
    summary.export_floor_eur_per_kwh = sale_floor;
    summary.zero_export = policy.zero_export;
    summary.cost_basis_eur_per_kwh = basis;
    summary.peak_reserve_kwh = policy.peak_target_kwh(aggregate.capacity_kwh, aggregate.start_kwh);
    summary.solar_reserve_kwh = policy.solar_reserve_kwh(aggregate.capacity_kwh);
    summary.clipped_kwh = outcome.clipped_kwh;
    summary.gated_kwh = 0.0;
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
        objective_eur: summary.cost_estimate_eur,
        sell_floor: sale_floor,
        export_floor_eur_per_kwh: summary.export_floor_eur_per_kwh,
        cost_basis_eur_per_kwh: basis,
        peak_reserve_kwh: policy.peak_target_kwh(aggregate.capacity_kwh, aggregate.start_kwh),
        solar_reserve_kwh: policy.solar_reserve_kwh(aggregate.capacity_kwh),
        clipped_kwh: outcome.clipped_kwh,
        gated_kwh: 0.0,
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

/// Clamp the solver's path with the policy's ceilings and export gate.
fn clamp_solved(
    input: &PlanInput,
    aggregate: &Aggregate,
    policy: Policy,
    solved: &energy_opt::SolveResult,
) -> clamp::Outcome {
    clamp::clamp(&Request {
        powers: &solved.powers,
        energies: &solved.energies,
        pv_kw: &input.pv_kw,
        load_kw: &input.load_kw,

        spec: aggregate.spec,
        policy,
    })
}

/// Append the clamp's bound to each row's reason, so the plan says why a slot moved.
fn with_clamp_reasons(
    rows: Vec<energy_ledger::plan::PlanSlot>,
    flags: &[clamp::Flags],
) -> Vec<energy_ledger::plan::PlanSlot> {
    rows.into_iter()
        .enumerate()
        .map(|(index, mut row)| {
            let suffix = flags.get(index).copied().unwrap_or_default().reason();
            if !suffix.is_empty() {
                if row.reason.is_empty() {
                    suffix.clone_into(&mut row.reason);
                } else {
                    row.reason = format!("{}; {suffix}", row.reason);
                }
            }
            row
        })
        .collect()
}

/// The cost basis: the live pool's value, or the no-export plan's own charging price while the pool
/// has no history yet.
fn resolve_basis(
    input: &PlanInput,
    slots: &[Slot],
    aggregate: &Aggregate,
) -> Result<Option<f64>, PlanError> {
    let basis = input.policy.cost_basis_eur_per_kwh;
    if basis.is_some() || input.policy.export != ExportPolicy::CostBasis {
        return Ok(basis);
    }
    presolve_basis(input, slots, aggregate)
}

/// The average price the no-export plan pays for the energy it charges, EUR per AC kWh.
fn presolve_basis(
    input: &PlanInput,
    slots: &[Slot],
    aggregate: &Aggregate,
) -> Result<Option<f64>, PlanError> {
    // The basis is "what the energy would cost if nothing were ever exported", so this pre-solve
    // must not export -- `solver_config` now allows export whenever the operator does.
    let mut config = config::solver_config(input, aggregate, 0.0);
    config.allow_export = false;
    let solved = solve(input, slots, &config, None)?;
    let mut energy = 0.0;
    let mut cost = 0.0;
    for (index, slot) in slots.iter().enumerate() {
        let power = solved.powers.get(index).copied().unwrap_or(0.0);
        if power <= 0.0 {
            continue;
        }
        let surplus = (input.pv_kw[index] - input.load_kw[index]).max(0.0);
        let pv_part = power.min(surplus) * SLOT_MINUTES / 60.0;
        let grid_part = (power * SLOT_MINUTES / 60.0 - pv_part).max(0.0);
        energy += pv_part + grid_part;
        cost += grid_part * slot.buy_all_in
            + pv_part * input.settlement.export_price(slot, ExportSource::Battery);
    }
    Ok((energy > 0.0).then(|| cost / energy))
}

/// The only export ban left: the operator's sentinel, reported as the floor.
fn sale_floor(policy: &Policy) -> Option<f64> {
    policy.zero_export.then_some(Policy::ZERO_EXPORT_FLOOR)
}

/// What a stored kWh left at the end of the horizon is worth, EUR/kWh of stored energy.
///
/// The cheapest AC price anywhere in the horizon, divided by the charge efficiency: that is what it
/// would cost to put the kWh back. Without it the solver will happily sell energy it never replaces
/// — the plan looks profitable and the house buys the same energy back tomorrow.
fn salvage(input: &PlanInput, slots: &[Slot], aggregate: &Aggregate) -> f64 {
    let eta = (aggregate.eta_charge * aggregate.eta_discharge).max(0.05);
    let cheapest = slots
        .iter()
        .map(|slot| slot.buy_all_in)
        .filter(|price| price.is_finite())
        .fold(f64::INFINITY, f64::min);
    if cheapest.is_finite() {
        cheapest / eta + input.wear_eur_per_kwh
    } else {
        0.0
    }
}

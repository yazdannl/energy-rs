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
    let floor = sell_floor(input, &slots);
    let basis = resolve_basis(input, &slots, &aggregate)?;
    let policy = Policy {
        cost_basis_eur_per_kwh: basis,
        ..input.policy
    };
    let break_even = export_floor(input, &slots, &aggregate, &policy);
    let config = config::solver_config(input, &aggregate, floor, break_even);
    let solved = solve(input, &slots, &config, check)?;
    let outcome = clamp_solved(input, &slots, &aggregate, policy, break_even, &solved);
    let rows = timeline::timeline(
        input,
        &slots,
        &outcome.energies,
        &outcome.powers,
        (aggregate.eta_charge, aggregate.eta_discharge),
    )?;
    let rows = with_clamp_reasons(rows, &outcome.flags);
    let mut summary = summary::summarize(&rows, aggregate.capacity_kwh, config.e_target);
    summary.export_floor_eur_per_kwh =
        break_even.filter(|floor| *floor < Policy::ZERO_EXPORT_FLOOR);
    summary.zero_export = policy.zero_export;
    summary.cost_basis_eur_per_kwh = basis;
    summary.peak_reserve_kwh = policy.peak_target_kwh(aggregate.capacity_kwh, aggregate.start_kwh);
    summary.solar_reserve_kwh = policy.solar_reserve_kwh(aggregate.capacity_kwh);
    summary.clipped_kwh = outcome.clipped_kwh;
    summary.gated_kwh = outcome.gated_kwh;
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
        sell_floor: Some(break_even.unwrap_or(floor)),
        export_floor_eur_per_kwh: summary.export_floor_eur_per_kwh,
        cost_basis_eur_per_kwh: basis,
        peak_reserve_kwh: policy.peak_target_kwh(aggregate.capacity_kwh, aggregate.start_kwh),
        solar_reserve_kwh: policy.solar_reserve_kwh(aggregate.capacity_kwh),
        clipped_kwh: outcome.clipped_kwh,
        gated_kwh: outcome.gated_kwh,
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
    slots: &[Slot],
    aggregate: &Aggregate,
    policy: Policy,
    break_even: Option<f64>,
    solved: &energy_opt::SolveResult,
) -> clamp::Outcome {
    let feed_in: Vec<f64> = slots
        .iter()
        .map(|slot| input.settlement.export_price(slot, ExportSource::Battery))
        .collect();
    clamp::clamp(&Request {
        powers: &solved.powers,
        energies: &solved.energies,
        pv_kw: &input.pv_kw,
        load_kw: &input.load_kw,
        feed_in: &feed_in,
        spec: aggregate.spec,
        policy,
        break_even,
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
    if basis.is_some() || input.policy.export != ExportPolicy::CostBasis || netting_on(input, slots)
    {
        return Ok(basis);
    }
    presolve_basis(input, slots, aggregate)
}

/// The export gate in force: the break-even when the policy is armed and netting is off.
///
/// While netting is on an exported kWh is netted 1:1 against imports, so the settlement's marginal
/// value is the right number and the gate stays inert.
fn export_floor(
    input: &PlanInput,
    slots: &[Slot],
    aggregate: &Aggregate,
    policy: &Policy,
) -> Option<f64> {
    if policy.zero_export {
        return Some(Policy::ZERO_EXPORT_FLOOR);
    }
    if policy.export != ExportPolicy::CostBasis || netting_on(input, slots) {
        return None;
    }
    policy.break_even(aggregate.eta_charge * aggregate.eta_discharge)
}

/// Whether 1:1 netting applies to this horizon: an exported kWh is then worth the all-in price and
/// the export gate is inert.
fn netting_on(input: &PlanInput, slots: &[Slot]) -> bool {
    slots
        .first()
        .is_some_and(|slot| input.settlement.saldering_active(slot))
}

/// The average price the no-export plan pays for the energy it charges, EUR per AC kWh.
fn presolve_basis(
    input: &PlanInput,
    slots: &[Slot],
    aggregate: &Aggregate,
) -> Result<Option<f64>, PlanError> {
    let config = config::solver_config(input, aggregate, 0.0, None);
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

//! The reference one-step evaluation and the strict argmin the decisions must match.

use super::Slot;
use super::cost::{interp, next_energy, slot_cost, wear_cost};
use super::window::actions;
use crate::config::Config;

/// The inputs the reference one-step evaluation needs for one slot.
///
/// Grouping them keeps the call sites honest: a bare argument list of this length is exactly how
/// the wrong row or grid gets passed in.
#[derive(Debug, Clone, Copy)]
pub struct SlotContext<'a> {
    /// The solver configuration (action count, prices, efficiencies).
    pub config: &'a Config,
    /// The slot being evaluated.
    pub slot: Slot,
    /// The slot's modelled duration, hours.
    pub dt_hours: f64,
    /// Row `i + 1` of the value function.
    pub next_row: &'a [f64],
    /// The state grid the row is defined on.
    pub grid: &'a [f64],
}

impl<'a> SlotContext<'a> {
    /// A context for one slot and its continuation row.
    pub fn new(
        config: &'a Config,
        slot: Slot,
        dt_hours: f64,
        next_row: &'a [f64],
        grid: &'a [f64],
    ) -> Self {
        Self {
            config,
            slot,
            dt_hours,
            next_row,
            grid,
        }
    }
}

/// The reference one-step cost of `action` at `state`: `(total, slot cost, wear cost)`.
///
/// `total` is what the sweep minimised: the slot's grid cost, the wear of the transition and the
/// interpolated next-row value.  Every operand is checked finite, exactly like `_fast.one_step`.
fn one_step(ctx: &SlotContext<'_>, state: f64, action: f64) -> Option<(f64, f64, f64)> {
    let e_next = next_energy(ctx.config, state, action, ctx.dt_hours);
    if !e_next.is_finite() {
        return None;
    }
    let step = slot_cost(ctx.config, &ctx.slot, ctx.dt_hours, action);
    let wear = wear_cost(ctx.config, state, e_next);
    let future = interp(ctx.next_row, ctx.grid, e_next);
    if !(step.is_finite() && wear.is_finite() && future.is_finite()) {
        return None;
    }
    let total = step + wear + future;
    total.is_finite().then_some((total, step, wear))
}

/// The reference strict argmin at `state` over `[lo, hi]`: `(power, cost)`.
///
/// Ties resolve to the lowest power (the action list is sorted and the comparison is strict), so
/// this is the decision the C core stored; `None` means the action set was empty or produced a
/// non-finite cost, which is itself a refusal.
pub fn argmin_at(
    ctx: &SlotContext<'_>,
    state: f64,
    lo: f64,
    hi: f64,
    extra: Option<f64>,
) -> Option<(f64, f64)> {
    let mut best_cost = f64::INFINITY;
    let mut best_power: Option<f64> = None;
    let align = ctx
        .config
        .align_actions
        .then(|| (ctx.slot.surplus(), -ctx.slot.base()));
    for action in actions(lo, hi, ctx.config.power_steps, extra, align) {
        let (total, _, _) = one_step(ctx, state, action)?;
        if total < best_cost {
            best_cost = total;
            best_power = Some(action);
        }
    }
    best_power.map(|power| (power, best_cost))
}

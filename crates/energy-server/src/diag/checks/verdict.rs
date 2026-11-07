//! The one-line answer: at this moment, would the engine plan a move or hold, and why.

use std::fmt::Write as _;

use energy_core::plan::Plan;

use crate::diag::check::Check;
use crate::diag::plan_probe::PlanProbe;

/// PASS when a plan exists (a hold is a plan), FAIL when the plan is refused.
pub fn check(failures: usize, probe: Option<&PlanProbe>, now: i64) -> Check {
    if failures > 0 {
        return Check::fail(
            "verdict",
            format!("would hold: {failures} check(s) failed; fix them before trusting a plan"),
        );
    }
    let Some(probe) = probe else {
        return Check::warn(
            "verdict",
            "would hold: no plan attempted (no Home Assistant, dashboard or packs)",
        );
    };
    match &probe.plan {
        Err(error) => Check::fail("verdict", format!("would hold: {error}")),
        Ok(plan) => planned(plan, probe, now),
    }
}

fn planned(plan: &Plan, probe: &PlanProbe, now: i64) -> Check {
    let Some(slot) = plan.slot_at(now) else {
        return Check::warn(
            "verdict",
            format!(
                "would hold: the horizon ({} slots) does not cover now",
                plan.slots.len()
            ),
        );
    };
    let action = if slot.battery_kw > 1e-3 {
        format!("would plan: charge {:.0} W", slot.battery_kw * 1000.0)
    } else if slot.battery_kw < -1e-3 {
        format!("would plan: discharge {:.0} W", -slot.battery_kw * 1000.0)
    } else {
        "would hold: no battery move in this slot".to_owned()
    };
    let mut detail = format!(
        "{action} ({}), soc {:.2} kWh -> {:.2} kWh, buy {:.4}, feed-in {:.4} EUR/kWh, {} pack(s)",
        slot.reason, slot.soc_kwh, slot.target_soc_kwh, slot.buy_all_in, slot.feed_in, probe.packs,
    );
    if probe.profile_days > 0 {
        let _ = write!(detail, ", profile {} d", probe.profile_days);
    }
    for note in &probe.notes {
        let _ = write!(detail, "; {note}");
    }
    match &plan.degraded {
        Some(reason) => {
            let _ = write!(detail, "; degraded: {reason}");
            Check::warn("verdict", detail)
        }
        None => Check::pass("verdict", detail),
    }
}

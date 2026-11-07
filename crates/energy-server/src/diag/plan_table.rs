//! The plan-once output: one row per 15-minute slot plus the summary the UI shows.

use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use chrono_tz::Europe::Amsterdam;
use energy_core::plan::Plan;
use energy_ledger::PlanSlot;
use serde_json::{Value, json};

use crate::diag::table;

/// The header (horizon, money, degradation) followed by the slot table.
pub fn render(plan: &Plan) -> String {
    let mut out = header(plan);
    let rows = rows(plan);
    let widths = table::widths(&rows, 3);
    for row in &rows {
        out.push_str(&table::line(row, &widths));
        out.push('\n');
    }
    out
}

/// The same plan as a JSON object, for scripting.
pub fn to_json(plan: &Plan) -> Value {
    let slots: Vec<Value> = plan
        .slots
        .iter()
        .map(|slot| {
            json!({
                "tStart": slot.t_start,
                "buyEurPerKwh": slot.buy_all_in,
                "feedInEurPerKwh": slot.feed_in,
                "pvKw": slot.pv_kw,
                "loadKw": slot.load_kw,
                "batteryW": slot.battery_kw * 1000.0,
                "socKwh": slot.soc_kwh,
                "reason": slot.reason,
            })
        })
        .collect();
    json!({
        "generatedAt": plan.generated_at,
        "horizonStart": plan.horizon_start,
        "horizonEnd": plan.horizon_end,
        "feasible": plan.feasible,
        "objectiveEur": plan.objective_eur,
        "sellFloorEurPerKwh": plan.sell_floor,
        "summary": plan.summary,
        "degraded": plan.degraded,
        "slots": slots,
    })
}

fn header(plan: &Plan) -> String {
    let summary = &plan.summary;
    let mut out = format!(
        "plan {} .. {} ({} slots), objective {:.6} EUR\n",
        local(plan.horizon_start),
        local(plan.horizon_end),
        plan.slots.len(),
        plan.objective_eur,
    );
    let _ = writeln!(
        out,
        "cost {:.6}, baseline {:.6}, saved {:.6} EUR; charged {:.2}, discharged {:.2} kWh, {:.2} cycles",
        summary.cost_estimate_eur,
        summary.baseline_cost_eur,
        summary.saved_eur,
        summary.charged_kwh,
        summary.discharged_kwh,
        summary.cycles,
    );
    let _ = writeln!(
        out,
        "grid charge {:.2}, PV charge {:.2} kWh, terminal target {}",
        summary.grid_charge_kwh,
        summary.pv_charge_kwh,
        summary
            .terminal_target_kwh
            .map_or_else(|| "-".to_owned(), |target| format!("{target:.2} kWh")),
    );
    if plan.feasible {
        out.push_str("feasible: yes\n");
    } else {
        let _ = writeln!(
            out,
            "feasible: no ({})",
            summary
                .infeasible_reason
                .as_deref()
                .unwrap_or("target unreachable")
        );
    }
    match &plan.degraded {
        Some(reason) => {
            let _ = writeln!(out, "degraded: {reason}");
        }
        None => out.push_str("degraded: none (fresh forecast)\n"),
    }
    out
}

fn rows(plan: &Plan) -> Vec<Vec<String>> {
    let mut rows = vec![vec![
        "SLOT".to_owned(),
        "BUY".to_owned(),
        "FEED-IN".to_owned(),
        "PV kW".to_owned(),
        "LOAD kW".to_owned(),
        "BATTERY W".to_owned(),
        "SOC kWh".to_owned(),
        "REASON".to_owned(),
    ]];
    rows.extend(plan.slots.iter().map(row));
    rows
}

fn row(slot: &PlanSlot) -> Vec<String> {
    vec![
        local(slot.t_start),
        format!("{:.5}", slot.buy_all_in),
        format!("{:.5}", slot.feed_in),
        format!("{:.2}", slot.pv_kw),
        format!("{:.2}", slot.load_kw),
        format!("{:+.0}", slot.battery_kw * 1000.0),
        format!("{:.2}", slot.soc_kwh),
        slot.reason.clone(),
    ]
}

fn local(epoch: i64) -> String {
    DateTime::<Utc>::from_timestamp(epoch, 0).map_or_else(
        || epoch.to_string(),
        |utc| {
            utc.with_timezone(&Amsterdam)
                .format("%m-%d %H:%M")
                .to_string()
        },
    )
}

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
#![allow(dead_code)]

//! Shared, deterministic fixtures for the M1/M3 scenario tests.

use energy_decide::horizon::{
    BudgetState, GridSource, Horizon, HorizonInput, HorizonOutcome, NetMeteringSchedule, PriceSlot,
    SlotSettlement, assemble_horizon,
};
use energy_decide::plan_assemble::{Bounds, PlanInput, Targets, TimelineSlot, bounds, targets};

pub fn ts(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value).map_or(0, |stamp| stamp.timestamp())
}

pub fn price(start: &str, buy: f64) -> PriceSlot {
    PriceSlot {
        t_start: ts(start),
        slot_seconds: 900,
        buy_all_in: buy,
        feed_in_post: 0.18,
    }
}

pub fn prices(start: &str, count: usize, buy: f64) -> Vec<PriceSlot> {
    let first = ts(start);
    (0..count)
        .map(|index| PriceSlot {
            t_start: first + i64::try_from(index).unwrap_or(i64::MAX) * 900,
            slot_seconds: 900,
            buy_all_in: buy,
            feed_in_post: 0.18,
        })
        .collect()
}

pub fn full_sources(count: usize) -> Vec<GridSource> {
    ["grid_import", "grid_export", "pv"]
        .iter()
        .map(|category| GridSource {
            source_id: format!("fixture:{category}"),
            category,
            required: true,
            available: true,
            slots: vec![Some(1.0); count],
        })
        .collect()
}

pub fn input(now: &str, slots: Vec<PriceSlot>) -> HorizonInput {
    let count = slots.len();
    HorizonInput {
        now: ts(now),
        requested_slots: 120,
        slots,
        published_at: ts(now) - 60,
        price_max_age_s: 21_600,
        forecast_available: true,
        grid_sources: full_sources(count.max(96)),
        net_metering: "date_driven".to_owned(),
        operator_override: None,
        budget: BudgetState::Unknown,
    }
}

pub fn assembled(now: &str, count: usize) -> Horizon {
    let attempt = assemble_horizon(&input(now, prices("2026-11-22T15:45:00Z", count, 0.4)));
    attempt
        .horizon()
        .cloned()
        .unwrap_or_else(|| panic!("fixture horizon must assemble: {:?}", attempt.refusal()))
}

pub fn bare_horizon(t0: i64, settlements: Vec<SlotSettlement>) -> Horizon {
    let slots = settlements
        .iter()
        .map(|row| PriceSlot {
            t_start: row.t_start,
            slot_seconds: 900,
            buy_all_in: row.buy_eur_per_kwh,
            feed_in_post: row.export_eur_per_kwh,
        })
        .collect();
    Horizon {
        t0,
        requested_slots: settlements.len(),
        coverage: energy_decide::horizon::Coverage {
            requested_slots: settlements.len(),
            actual_slots: settlements.len(),
            minimum_slots: 96,
            coverage_end: Some(t0 + i64::try_from(settlements.len()).unwrap_or(i64::MAX) * 900),
            missing_slots: 0,
            first_missing_t_start: None,
        },
        slots,
        net_metering: NetMeteringSchedule { slots: settlements },
        grid_series: Vec::new(),
        grid_source_coverage: Vec::new(),
        outcome: HorizonOutcome::Healthy,
    }
}

pub fn settlement(t_start: i64, buy: f64, export: f64, netting: bool) -> SlotSettlement {
    SlotSettlement {
        t_start,
        buy_eur_per_kwh: buy,
        export_eur_per_kwh: export,
        netting,
        reason: energy_decide::horizon::SettlementReason::SalderingBudgetUnknown,
        budget_source: "unknown",
    }
}

pub fn flat(t_start: i64, e_kwh: f64, battery_w: f64, grid_w: f64) -> TimelineSlot {
    TimelineSlot {
        t_start,
        e_start_kwh: e_kwh,
        e_end_kwh: e_kwh,
        battery_w,
        grid_w,
        pv_w: 0.0,
        load_w: 0.0,
        pack_powers: Vec::new(),
        cell_ok: true,
        thermal_ok: true,
        pv_headroom_kwh: 10.0,
    }
}

pub fn plan_input(horizon: Horizon, slots: Vec<TimelineSlot>) -> PlanInput {
    PlanInput {
        generated_at: horizon.t0,
        horizon,
        slots,
        bounds: bounds::fleet_bounds(),
        targets: targets::fleet_targets(),
        obligations: Vec::new(),
        refusal: None,
        acceptance: None,
        delivery: energy_decide::plan_assemble::Delivery::default(),
    }
}

pub fn custom_plan_input(
    horizon: Horizon,
    slots: Vec<TimelineSlot>,
    bounds: Bounds,
    targets: Targets,
) -> PlanInput {
    PlanInput {
        generated_at: horizon.t0,
        horizon,
        slots,
        bounds,
        targets,
        obligations: Vec::new(),
        refusal: None,
        acceptance: None,
        delivery: energy_decide::plan_assemble::Delivery::default(),
    }
}

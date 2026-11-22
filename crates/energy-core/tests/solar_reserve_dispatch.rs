//! Dispatch-time proof of the solar-reserve verdict: the release follows the plan's remaining
//! forecast, maintenance is exempt from the reserve bound only, and the import ceiling still caps
//! the *sum* of planner and maintenance charge.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_core::dispatch::{
    Dispatcher, FleetView, ManualOverride, MeterReading, OverrideAction, Overrides, PackReading,
    TickInput,
};
use energy_core::plan::Plan;
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{FakeSink, NOW, pack, plan};

fn limits(import_w: f64) -> SafetyLimits {
    SafetyLimits {
        max_grid_import_w: import_w,
        grid_import_margin_w: 0.0,
        solar_reserve_pct: 10.0,
        grid_follow_deadband_w: 1.0e9, // keep the meter loop inert; these are ladder tests
        ..SafetyLimits::default()
    }
}

/// Current slot has no PV surplus; slot 1 promises one. `pv_now` is what the meter sees this tick.
fn forecast_plan(battery_kw: f64, pv_now: f64, pv_later: f64, soc_kwh: f64) -> Plan {
    let mut result = plan(NOW, battery_kw, soc_kwh);
    for (index, slot) in result.slots.iter_mut().enumerate() {
        slot.load_kw = 0.65;
        slot.pv_kw = if index == 0 { pv_now } else { pv_later };
        slot.soc_kwh = soc_kwh;
        slot.target_soc_kwh = soc_kwh + battery_kw * 0.25 * 0.95;
    }
    result
}

fn meter(pv_kw: f64) -> MeterReading {
    MeterReading {
        grid_w: Some(450.0),
        load_kw: Some(0.65),
        pv_kw: Some(pv_kw),
        observed_at: Some(NOW),
    }
}

struct TickCase<'a> {
    plan: Option<&'a Plan>,
    packs: Vec<PackReading>,
    overrides: &'a Overrides,
    limits: &'a SafetyLimits,
    meter: &'a MeterReading,
    now: i64,
    stale: bool,
}

async fn tick(case: TickCase<'_>) -> (energy_core::dispatch::TickOutcome, FakeSink) {
    let dispatcher = Dispatcher::new(case.limits.clone(), true, 10);
    let fleet = FleetView::new(case.packs);
    let latches = Vec::new();
    let sink = FakeSink::new();
    let input = TickInput {
        now: case.now,
        mode: Authority::Auto,
        plan: case.plan,
        fleet: &fleet,
        meter: case.meter,
        latches: &latches,
        overrides: case.overrides,
        stale: case.stale,
        progress: None,
        selection: None,
    };
    let outcome = dispatcher.tick(&input, &sink).await;
    (outcome, sink)
}

/// A pack above the 10 % solar ceiling: 94 % of 1.92 kWh = 1.8048 kWh, ceiling 1.728 kWh.
fn high_pack() -> PackReading {
    pack("SN-A", 94.0)
}

#[tokio::test]
async fn a_sunless_remaining_forecast_releases_the_reserve_at_dispatch() {
    let plan = forecast_plan(1.0, 0.0, 0.0, 1.8048);
    let meter = meter(0.0);
    let (outcome, _) = tick(TickCase {
        plan: Some(&plan),
        packs: vec![high_pack()],
        overrides: &Overrides::none(),
        limits: &limits(0.0),
        meter: &meter,
        now: NOW,
        stale: false,
    })
    .await;
    let report = outcome.report("SN-A").expect("report");
    assert!(
        report.intended_w > 0.0,
        "a sunless horizon must not throttle the planned charge: {report:?}"
    );
}

#[tokio::test]
async fn a_cloud_gap_mid_slot_keeps_the_reserve_armed() {
    let plan = forecast_plan(1.0, 1.5, 1.5, 1.8048);
    let meter = meter(0.0); // PV dropped to zero inside the slot
    let (outcome, _) = tick(TickCase {
        plan: Some(&plan),
        packs: vec![high_pack()],
        overrides: &Overrides::none(),
        limits: &limits(0.0),
        meter: &meter,
        now: NOW + 5,
        stale: false,
    })
    .await;
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(
        report.intended_w, 0.0,
        "cloud gap must not release the reserve"
    );
    assert_eq!(report.intended_reason, Reason::SocCeiling);
}

#[tokio::test]
async fn sunrise_keeps_the_reserve_armed_until_the_forecast_surplus_arrives() {
    // Current slot is still dark, but slot 1 forecasts surplus: the reserve stays armed.
    let plan = forecast_plan(1.0, 0.0, 1.5, 1.8048);
    let meter = meter(0.0);
    let (outcome, _) = tick(TickCase {
        plan: Some(&plan),
        packs: vec![high_pack()],
        overrides: &Overrides::none(),
        limits: &limits(0.0),
        meter: &meter,
        now: NOW,
        stale: false,
    })
    .await;
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(report.intended_w, 0.0, "reserve released before sunrise");
    assert_eq!(report.intended_reason, Reason::SocCeiling);
}

#[tokio::test]
async fn a_stale_plan_at_a_slot_boundary_never_releases_the_reserve() {
    let plan = forecast_plan(1.0, 0.0, 1.5, 1.8048);
    let meter = meter(0.0);
    let boundary = plan.slots[1].t_start;
    let (outcome, _) = tick(TickCase {
        plan: Some(&plan),
        packs: vec![high_pack()],
        overrides: &Overrides::none(),
        limits: &limits(0.0),
        meter: &meter,
        now: boundary,
        stale: true,
    })
    .await;
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(report.intended_w, 0.0, "stale plan started a charge");
}

#[tokio::test]
async fn maintenance_charge_survives_surplus_and_the_import_ceiling_caps_the_sum() {
    // One pack above the solar ceiling needs its seven-day balance; three packs below it follow
    // the planner's 2.4 kW charge. The measured PV gap makes the ordinary charge hit the reserve
    // ceiling, but the maintenance charge is exempt from that bound only.
    let mut packs = vec![high_pack()];
    for id in ["SN-B", "SN-C", "SN-D"] {
        packs.push(pack(id, 80.0));
    }
    let plan = forecast_plan(2.4, 0.2, 1.5, 4.0 * 1.8048);
    let overrides = Overrides::new(vec![ManualOverride::maintenance(
        "SN-A",
        OverrideAction::Charge { watts: 800.0 },
        NOW + 60,
    )]);
    let import_w = 2_000.0;
    let meter = meter(0.0);
    let (outcome, sink) = tick(TickCase {
        plan: Some(&plan),
        packs,
        overrides: &overrides,
        limits: &limits(import_w),
        meter: &meter,
        now: NOW + 5,
        stale: false,
    })
    .await;

    let maintenance = outcome.report("SN-A").expect("maintenance report");
    assert!(
        maintenance.intended_w > 0.0,
        "the solar ceiling starved the balance: {maintenance:?}"
    );
    assert_ne!(
        maintenance.intended_reason,
        Reason::SocCeiling,
        "the solar ceiling must not be the binding bound"
    );
    assert!(
        outcome
            .packs
            .iter()
            .filter(|report| report.pack_id != "SN-A")
            .any(|report| report.intended_w > 0.0),
        "the planner charge was blocked too"
    );
    // Meter base is 0.65 - 0.2 = 0.45 kW import, so only 1.55 kW is left for all four packs.
    let total_written: f64 = sink.writes().iter().sum();
    assert!(
        total_written <= import_w - 450.0 + 1.0,
        "planner + maintenance wrote {total_written} W over the {import_w} W ceiling"
    );
    assert!(total_written > 0.0, "no charge reached a device");
    assert_eq!(outcome.grid_limit, Some(Reason::GridImportLimit));
}

#[tokio::test]
async fn missing_pv_telemetry_blocks_charge_with_a_named_reason() {
    let plan = forecast_plan(1.0, 1.5, 1.5, 1.8048);
    let meter = MeterReading {
        pv_kw: None,
        ..meter(1.5)
    };
    let (outcome, _) = tick(TickCase {
        plan: Some(&plan),
        packs: vec![high_pack()],
        overrides: &Overrides::none(),
        limits: &limits(0.0),
        meter: &meter,
        now: NOW + 5,
        stale: false,
    })
    .await;
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(report.intended_w, 0.0, "missing PV must fail closed");
    assert_eq!(report.intended_reason, Reason::SolarPvTelemetryMissing);
}

#[tokio::test]
async fn missing_load_telemetry_blocks_charge_with_a_named_reason() {
    let plan = forecast_plan(1.0, 1.5, 1.5, 1.8048);
    let meter = MeterReading {
        load_kw: None,
        ..meter(1.5)
    };
    let (outcome, _) = tick(TickCase {
        plan: Some(&plan),
        packs: vec![high_pack()],
        overrides: &Overrides::none(),
        limits: &limits(0.0),
        meter: &meter,
        now: NOW + 5,
        stale: false,
    })
    .await;
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(report.intended_w, 0.0, "missing load must fail closed");
    assert_eq!(report.intended_reason, Reason::SolarLoadTelemetryMissing);
}

#[tokio::test]
async fn only_a_due_balance_may_charge_past_max_soc_pct() {
    // 94.9 % of a 1.92 kWh pack is 1.82208 kWh; its normal 95 % ceiling leaves 0.00192 kWh,
    // about 242.5 W at a 30 s tick and 0.95 efficiency. Economic charging must stop there.
    let plan = forecast_plan(1.0, 0.0, 0.0, 1.82208);
    let meter = meter(0.0);
    let near_full = pack("SN-A", 94.9);
    let (outcome, _) = tick(TickCase {
        plan: Some(&plan),
        packs: vec![near_full.clone()],
        overrides: &Overrides::none(),
        limits: &limits(0.0),
        meter: &meter,
        now: NOW + 5,
        stale: false,
    })
    .await;
    let economic = outcome.report("SN-A").expect("economic report");
    assert!(
        (economic.intended_w - 242.5).abs() < 2.0,
        "economic charge ignored max_soc_pct: {economic:?}"
    );
    assert!(
        economic.intended_w < 800.0,
        "economic request exceeded the normal ceiling"
    );

    // The same pack inside a scheduled, due seven-day balance may reach 100 % for cell balancing.
    let overrides = Overrides::new(vec![ManualOverride::maintenance(
        "SN-A",
        OverrideAction::Charge { watts: 800.0 },
        NOW + 60,
    )]);
    let (outcome, _) = tick(TickCase {
        plan: Some(&plan),
        packs: vec![near_full],
        overrides: &overrides,
        limits: &limits(0.0),
        meter: &meter,
        now: NOW + 5,
        stale: false,
    })
    .await;
    let balance = outcome.report("SN-A").expect("maintenance report");
    assert!(
        balance.intended_w > economic.intended_w + 100.0,
        "balancing was bounded by the normal SoC ceiling: {balance:?}"
    );
    assert_ne!(balance.intended_reason, Reason::SocCeiling);
}

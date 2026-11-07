//! One test per behavioural trap the clamp ladder must not lose (survey §7).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use chrono::{TimeZone, Utc};
use energy_battery::protection::{CellLatch, LatchReason};
use energy_core::allocation::Allocation;
use energy_core::plan::PackState;
use energy_core::safety::{Authority, GridFollow, Reason, SafetyLimits, check};
use support::{NOW, plan_slot, request, spec, state};

fn limits() -> SafetyLimits {
    SafetyLimits {
        tick_seconds: 30.0,
        fleet_capacity_kwh: 4.0,
        ..SafetyLimits::default()
    }
}

fn latch(force: bool) -> CellLatch {
    let at = Utc.timestamp_opt(0, 0).single().unwrap();
    CellLatch {
        pack_id: "a".to_owned(),
        blocked: true,
        force_charge: force,
        force_charge_w: Some(600),
        since: at,
        reason: LatchReason::BelowCutoff,
        reason_at: at,
        voltage_v: Some(2.9),
        last_fresh_at: Some(at),
    }
}

#[test]
fn authority_gate_comes_first_and_writes_nothing() {
    let pack = spec("a", 4.0, 1000.0);
    let advisory = SafetyLimits {
        mode: Authority::Advisory,
        ..limits()
    };
    let decision = check(&request("a", 500.0, NOW), &pack, &advisory, None);
    assert!(decision.blocked);
    assert_eq!(decision.allowed_w, 0.0);
    assert!(decision.has(Reason::ModeNotAuto));
    assert!(!decision.writes());
}

#[test]
fn a_dry_run_writes_nothing_including_the_exit_hold() {
    let pack = spec("a", 4.0, 1000.0);
    let dry = SafetyLimits {
        mode: Authority::DryRun,
        ..limits()
    };
    let hold = request("a", 0.0, NOW).as_exit_hold();
    let decision = check(&hold, &pack, &dry, None);
    assert!(decision.blocked);
    assert!(decision.has(Reason::ModeNotAuto));
    assert!(decision.has(Reason::ExitHoldSuppressed));
    assert_eq!(decision.allowed_w, 0.0);
}

#[test]
fn in_auto_a_neutral_exit_hold_is_allowed() {
    let pack = spec("a", 4.0, 1000.0);
    let decision = check(
        &request("a", 0.0, NOW).as_exit_hold(),
        &pack,
        &limits(),
        None,
    );
    assert!(!decision.blocked);
    assert_eq!(decision.allowed_w, 0.0);
}

#[test]
fn a_stale_pack_holds_instead_of_repeating_the_last_setpoint() {
    let pack = spec("a", 4.0, 1000.0);
    let mut stale = request("a", 500.0, NOW);
    stale.measured.stale = true;
    let decision = check(&stale, &pack, &limits(), None);
    assert!(decision.blocked);
    assert!(decision.has(Reason::StaleState));
}

#[test]
fn the_ramp_is_per_pack_after_allocation() {
    let packs = vec![state("a", 4.0, 2000.0, 50.0), state("b", 4.0, 2000.0, 50.0)];
    let split = Allocation::split(4000.0, &packs, 0.0, 30.0 / 3600.0, &[]);
    assert_eq!(split.watts_for("a"), Some(2000.0));
    assert_eq!(split.watts_for("b"), Some(2000.0));
    let mut limits = limits();
    limits.fleet_capacity_kwh = 8.0;
    for id in ["a", "b"] {
        let pack = spec(id, 4.0, 2000.0);
        let mut req = request(id, 2000.0, NOW);
        req.measured.energy_kwh = Some(2.0);
        req.measured.power_w = 0.0;
        let decision = check(&req, &pack, &limits, None);
        assert_eq!(decision.allowed_w, 1000.0, "{id} must ramp on its own");
        assert!(decision.has(Reason::RampLimit));
    }
}

#[test]
fn plan_tracking_cuts_the_setpoint_to_land_on_the_slot_target() {
    let pack = spec("a", 4.0, 1000.0);
    let mut req = request("a", 1000.0, NOW);
    req.measured.energy_kwh = Some(2.2);
    req.plan_slot = Some(plan_slot(NOW - 300, 1.0, 2.0));
    let decision = check(&req, &pack, &limits(), None);
    assert!(
        (decision.allowed_w - 236.84).abs() < 0.5,
        "{}",
        decision.allowed_w
    );
    assert!(decision.has(Reason::PlanTarget));
}

#[test]
fn plan_tracking_never_reverses_a_charge_into_a_discharge() {
    let pack = spec("a", 4.0, 1000.0);
    let mut req = request("a", 1000.0, NOW);
    req.measured.energy_kwh = Some(2.3);
    req.plan_slot = Some(plan_slot(NOW - 300, 1.0, 2.0));
    let decision = check(&req, &pack, &limits(), None);
    assert_eq!(decision.allowed_w, 0.0);
    assert!(decision.has(Reason::PlanTarget));
}

#[test]
fn a_hold_slot_is_not_tracked() {
    let pack = spec("a", 4.0, 1000.0);
    let mut req = request("a", 500.0, NOW);
    req.measured.energy_kwh = Some(2.0);
    req.plan_slot = Some(plan_slot(NOW - 300, 0.0, 2.0));
    let decision = check(&req, &pack, &limits(), None);
    assert_eq!(decision.allowed_w, 500.0);
    assert!(!decision.has(Reason::PlanTarget));
}

#[test]
fn a_missing_grid_meter_and_base_fails_closed() {
    let pack = spec("a", 4.0, 1000.0);
    let mut limits = limits();
    limits.max_grid_import_w = 1000.0;
    limits.max_grid_export_w = 1000.0;
    let mut req = request("a", 500.0, NOW);
    req.measured.energy_kwh = Some(2.0);
    let decision = check(&req, &pack, &limits, None);
    assert!(decision.blocked);
    assert!(decision.has(Reason::GridTelemetryMissing));
}

#[test]
fn an_unknown_state_blocks_a_charge_and_a_discharge() {
    let pack = spec("a", 4.0, 1000.0);
    // Neither stored energy nor SoC: the pack floor must not invent charge headroom.
    for requested in [500.0, -500.0] {
        let decision = check(&request("a", requested, NOW), &pack, &limits(), None);
        assert!(decision.blocked, "{requested} W was allowed with no state");
        assert_eq!(decision.allowed_w, 0.0);
        assert!(decision.has(Reason::StateMissing), "{:?}", decision.reasons);
        assert!(!decision.writes());
    }
}

#[test]
fn a_neutral_hold_is_allowed_with_an_unknown_state() {
    let pack = spec("a", 4.0, 1000.0);
    let decision = check(&request("a", 0.0, NOW), &pack, &limits(), None);
    assert!(!decision.blocked);
    assert_eq!(decision.allowed_w, 0.0);
    assert!(decision.has(Reason::StateMissing));
    assert!(!decision.writes());
}

#[test]
fn the_grid_cap_uses_the_forecast_base_when_the_meter_is_missing() {
    let pack = spec("a", 4.0, 1000.0);
    let mut limits = limits();
    limits.max_grid_import_w = 400.0;
    let mut req = request("a", 500.0, NOW);
    req.measured.energy_kwh = Some(2.0);
    let mut slot = plan_slot(NOW, 0.0, 2.0);
    slot.load_kw = 0.3;
    req.plan_slot = Some(slot);
    let decision = check(&req, &pack, &limits, None);
    assert_eq!(decision.allowed_w, 100.0);
    assert!(decision.has(Reason::GridImportLimit));
}

#[test]
fn the_meter_is_corrected_back_to_the_battery_idle_base() {
    let pack = spec("a", 4.0, 1000.0);
    let mut limits = limits();
    limits.max_grid_import_w = 400.0;
    let mut req = request("a", 500.0, NOW);
    req.measured.energy_kwh = Some(2.0);
    req.measured.grid_w = Some(1000.0);
    req.measured.fleet_power_w = 500.0;
    // Base = 1.0 - 0.5 = 0.5 kW, so at most -100 W may be added.
    let decision = check(&req, &pack, &limits, None);
    assert_eq!(decision.allowed_w, -100.0);
}

#[test]
fn the_sell_floor_judges_only_the_plans_own_discharge() {
    let pack = spec("a", 4.0, 1000.0);
    let mut limits = limits();
    limits.sell_floor_eur_per_kwh = Some(0.12);
    let mut discharge = request("a", -500.0, NOW);
    discharge.measured.energy_kwh = Some(2.0);
    let mut slot = plan_slot(NOW, -0.5, 2.0);
    slot.load_kw = 0.2;
    discharge.plan_slot = Some(slot);
    let decision = check(&discharge, &pack, &limits, None);
    assert_eq!(decision.allowed_w, 0.0);
    assert!(decision.has(Reason::SellFloor));
    let mut charge = request("a", 500.0, NOW);
    charge.measured.energy_kwh = Some(2.0);
    charge.plan_slot = Some(plan_slot(NOW, 0.5, 2.0));
    let decision = check(&charge, &pack, &limits, None);
    assert!((decision.allowed_w - 500.0).abs() < 1e-6);
    assert!(!decision.has(Reason::SellFloor));
}

#[test]
fn a_cell_voltage_latch_blocks_discharge_and_forces_a_charge() {
    let pack = spec("a", 4.0, 1000.0);
    let mut req = request("a", -1000.0, NOW);
    req.measured.energy_kwh = Some(2.0);
    let decision = check(&req, &pack, &limits(), Some(&latch(true)));
    assert_eq!(decision.allowed_w, 600.0);
    assert!(decision.has(Reason::CellVoltageGuard));
    assert!(decision.has(Reason::CellVoltageEmergencyCharge));
    let mut req = request("a", -500.0, NOW);
    req.measured.energy_kwh = Some(2.0);
    let decision = check(&req, &pack, &limits(), Some(&latch(false)));
    assert_eq!(decision.allowed_w, 0.0);
    assert!(decision.has(Reason::CellVoltageGuard));
}

#[test]
fn lo_above_hi_blocks_with_every_binding_reason() {
    let pack = spec("a", 4.0, 1000.0);
    let mut limits = limits();
    limits.sell_floor_eur_per_kwh = Some(0.12);
    limits.max_grid_import_w = 1000.0;
    let mut req = request("a", -500.0, NOW);
    req.measured.energy_kwh = Some(3.8);
    let mut slot = plan_slot(NOW, -0.5, 3.8);
    slot.load_kw = 5.0;
    slot.battery_kw = -5.5;
    req.plan_slot = Some(slot);
    let decision = check(&req, &pack, &limits, None);
    assert!(decision.blocked);
    assert!(decision.reasons.len() >= 2, "{:?}", decision.reasons);
    assert!(decision.has(Reason::GridImportLimit));
    assert!(decision.has(Reason::SellFloor));
}

#[test]
fn the_grid_follow_deadband_and_budget_are_pinned() {
    let follow = GridFollow::default();
    assert_eq!(follow.correct(0.0, 20.0, 0.0, 1000.0), 0.0);
    assert_eq!(follow.correct(0.0, 100.0, 0.0, 1000.0), -100.0);
    // Capability 600 W -> 300 W, below the 500 W floor.
    assert_eq!(follow.correct(0.0, 2000.0, 0.0, 600.0), -500.0);
    assert_eq!(follow.correct(0.0, 2000.0, 0.0, 4000.0), -2000.0);
}

#[test]
fn allocation_respects_each_pack_ceiling_before_the_ramp() {
    let packs = vec![PackState {
        spec: spec("a", 4.0, 300.0),
        soc_pct: 50.0,
    }];
    let split = Allocation::split(-5000.0, &packs, 0.0, 0.25, &[]);
    assert_eq!(split.watts_for("a"), Some(-300.0));
    assert_eq!(split.unmet_w, -4700.0);
}

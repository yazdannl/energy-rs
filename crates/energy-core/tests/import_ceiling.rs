//! The reactive import ceiling: measured import breaches the limit, the fleet serves it.
//!
//! These are dispatcher-level tests through `FakeSink`: the plan, the grid follow, the allocation,
//! the ladder and the final-vector clamp all run for real. The important distinction is between the
//! fleet *target* (what the ceiling asks for) and the watts that reach a device after the pack
//! bounds have had their say.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::assert_is_empty
)]

mod support;

use chrono::{TimeZone, Utc};
use energy_battery::protection::{CellLatch, LatchReason};
use energy_core::dispatch::{
    Dispatcher, FleetView, MeterReading, Overrides, PackReading, TickInput, TickOutcome,
};
use energy_core::plan::Plan;
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{FakeSink, NOW, pack, plan};

/// A Plan slot that deliberately holds the battery, with buy and feed-in priced 1:1.
///
/// This is the netted-hold case `follow_target` refuses to correct: without the ceiling guard the
/// fleet stays idle at 0 W even while the house imports 7.6 kW.
fn netted_plan(battery_kw: f64, load_kw: f64) -> Plan {
    let mut plan = plan(NOW, battery_kw, 1.92);
    for slot in &mut plan.slots {
        slot.buy_all_in = 0.30;
        slot.feed_in = 0.30;
        slot.load_kw = load_kw;
        slot.pv_kw = 0.0;
    }
    plan
}

fn limits() -> SafetyLimits {
    SafetyLimits {
        max_grid_import_w: 7_700.0,
        grid_import_margin_w: 200.0,
        reserve_soc_pct: 10.0,
        ..SafetyLimits::default()
    }
}

fn written_w(sink: &FakeSink) -> f64 {
    sink.writes().iter().sum()
}

fn latch(pack_id: &str) -> CellLatch {
    let at = Utc.timestamp_opt(0, 0).single().expect("epoch");
    CellLatch {
        pack_id: pack_id.to_owned(),
        blocked: true,
        force_charge: false,
        force_charge_w: None,
        since: at,
        reason: LatchReason::BelowCutoff,
        reason_at: at,
        voltage_v: Some(3.15),
        last_fresh_at: Some(at),
    }
}

/// Everything one tick needs, mutable so a test can move the meter between ticks.
struct Scenario {
    plan: Plan,
    packs: Vec<PackReading>,
    latches: Vec<CellLatch>,
    limits: SafetyLimits,
    meter: MeterReading,
    mode: Authority,
    stale: bool,
    /// Simulates a tick with no stored plan at all (distinct from an expired one).
    no_plan: bool,
    /// Shared across ticks so the guard's hysteresis and the fault backoff survive between them.
    dispatcher: Option<Dispatcher>,
}

impl Scenario {
    /// A netted hold at 0 W with two idle packs at 50 % SoC.
    fn hold(grid_w: f64) -> Self {
        Self {
            plan: netted_plan(0.0, 0.0),
            packs: vec![pack("SN-A", 50.0), pack("SN-B", 50.0)],
            latches: Vec::new(),
            limits: limits(),
            meter: MeterReading::grid(grid_w),
            mode: Authority::Auto,
            stale: false,
            no_plan: false,
            dispatcher: None,
        }
    }

    fn set_battery_power(&mut self, pack_id: &str, watts: f64) {
        self.packs
            .iter_mut()
            .find(|reading| reading.pack_id() == pack_id)
            .expect("pack")
            .power_w = watts;
    }

    async fn run(&mut self) -> (TickOutcome, FakeSink) {
        // A test that changes the limits gets a fresh dispatcher; a test that moves only the meter
        // keeps the same one, which is what a multi-tick hysteresis check needs.
        if self
            .dispatcher
            .as_ref()
            .is_none_or(|dispatcher| dispatcher.limits() != &self.limits)
        {
            self.dispatcher = Some(Dispatcher::new(self.limits.clone(), true, 10));
        }
        let dispatcher = self.dispatcher.as_ref().expect("dispatcher");
        let fleet = FleetView::new(self.packs.clone());
        let overrides = Overrides::none();
        let sink = FakeSink::new();
        // Production stamps the meter at fetch time; these ticks read meter and packs together.
        let meter = MeterReading {
            observed_at: Some(NOW),
            ..self.meter
        };
        let input = TickInput {
            now: NOW,
            mode: self.mode,
            plan: (!self.no_plan).then_some(&self.plan),
            fleet: &fleet,
            meter: &meter,
            latches: &self.latches,
            overrides: &overrides,
            stale: self.stale,
            progress: None,
            selection: None,
        };
        let outcome = dispatcher.tick(&input, &sink).await;
        (outcome, sink)
    }
}

#[tokio::test]
async fn a_measured_breach_dispatches_the_exact_ceiling_setpoint() {
    // 7600 W import, 7700 W limit, 200 W margin -> hold 7500 W -> serve 100 W from the fleet.
    let mut scenario = Scenario::hold(7_600.0);
    let (outcome, sink) = scenario.run().await;
    assert_eq!(outcome.target_w, -100.0, "fleet target must be -100 W");
    assert_eq!(
        sink.writes(),
        vec![-100.0],
        "one pack covers the whole breach"
    );
    let writer = outcome
        .packs
        .iter()
        .find(|row| row.intended_w < -0.5)
        .expect("writer");
    assert_eq!(writer.reason, Reason::GridImportCeiling);
}

#[tokio::test]
async fn a_netted_zero_watt_hold_slot_is_discharged_into_when_the_ceiling_is_breached() {
    // The plan's 0 W hold and the netted-hold skip both say "do nothing"; the ceiling is safety and
    // runs above both. A plan charge of 7.6 kW would be a different matter: the meter is the input.
    let mut scenario = Scenario::hold(7_690.0);
    let (outcome, sink) = scenario.run().await;
    assert_eq!(outcome.plan_power_w, Some(0.0));
    assert_eq!(outcome.target_w, -190.0);
    assert_eq!(sink.writes(), vec![-190.0]);
    assert!(
        outcome
            .packs
            .iter()
            .filter(|row| row.intended_w < -0.5)
            .all(|row| row.reason == Reason::GridImportCeiling),
        "{:?}",
        outcome.packs
    );
}

#[tokio::test]
async fn import_below_the_target_leaves_the_plan_alone() {
    let mut scenario = Scenario::hold(7_400.0);
    let (outcome, sink) = scenario.run().await;
    assert_eq!(outcome.target_w, 0.0, "7400 W is below the 7500 W target");
    assert!(sink.writes().is_empty(), "no guessed discharge");
    assert_eq!(sink.holds(), 2);
}

#[tokio::test]
async fn a_zero_margin_holds_exactly_at_the_limit() {
    let mut scenario = Scenario::hold(7_705.0);
    scenario.limits.grid_import_margin_w = 0.0;
    let (outcome, sink) = scenario.run().await;
    assert_eq!(
        outcome.target_w, 0.0,
        "5 W over the limit is inside the deadband"
    );
    assert!(sink.writes().is_empty(), "inside the deadband is noise");

    scenario.meter = MeterReading::grid(7_800.0);
    let (outcome, sink) = scenario.run().await;
    assert_eq!(
        outcome.target_w, -100.0,
        "target import is the 7700 W limit"
    );
    assert_eq!(sink.writes(), vec![-100.0]);
}

#[tokio::test]
async fn a_zero_import_limit_disables_the_guard() {
    let mut scenario = Scenario::hold(9_000.0);
    scenario.limits.max_grid_import_w = 0.0;
    let (outcome, sink) = scenario.run().await;
    assert_eq!(outcome.target_w, 0.0, "no configured limit, no ceiling");
    assert!(sink.writes().is_empty());
    assert_eq!(sink.holds(), 2, "the plan's 0 W hold stands");
}

#[tokio::test]
async fn the_soc_floor_leaves_nothing_to_give() {
    let mut scenario = Scenario::hold(7_600.0);
    scenario.packs = vec![pack("SN-A", 10.0), pack("SN-B", 10.0)];
    let (outcome, sink) = scenario.run().await;
    assert_eq!(outcome.target_w, -100.0, "the guard still sees the breach");
    assert!(sink.writes().is_empty(), "the reserve is not a battery");
    assert!(outcome.packs.iter().all(|row| row.intended_w == 0.0));
}

#[tokio::test]
async fn a_latched_cell_is_never_discharged_by_the_guard() {
    let mut scenario = Scenario::hold(7_600.0);
    scenario.latches = vec![latch("SN-A")];
    let (outcome, sink) = scenario.run().await;
    assert_eq!(outcome.target_w, -100.0);
    assert_eq!(
        sink.writes(),
        vec![-100.0],
        "the free pack serves the breach"
    );
    let blocked = outcome.report("SN-A").expect("latched row");
    assert_eq!(blocked.intended_w, 0.0, "the latched pack stays out of it");
    assert_ne!(blocked.reason, Reason::GridImportCeiling);
}

#[tokio::test]
async fn hysteresis_holds_the_discharge_until_the_meter_is_clearly_below_target() {
    let mut scenario = Scenario::hold(7_600.0);
    let (first, first_sink) = scenario.run().await;
    assert_eq!(first.target_w, -100.0);
    assert_eq!(first_sink.writes(), vec![-100.0]);

    // The device answered: the meter is now at the target (not below it) and the pack serves 100 W.
    scenario.meter = MeterReading::grid(7_500.0);
    scenario.set_battery_power("SN-A", -100.0);
    let (second, second_sink) = scenario.run().await;
    assert_eq!(
        second.target_w, -100.0,
        "releasing here would flap next tick"
    );
    assert_eq!(second_sink.writes(), vec![-100.0]);

    // A genuine load drop makes the replacement safe: the old plan comes back.
    scenario.meter = MeterReading::grid(7_380.0);
    let (third, third_sink) = scenario.run().await;
    assert_eq!(third.target_w, 0.0);
    assert!(
        third_sink.writes().is_empty(),
        "the hold slot is a hold again"
    );
}

#[tokio::test]
async fn a_grid_charge_slot_is_stopped_when_it_would_breach_the_ceiling() {
    let mut scenario = Scenario::hold(7_600.0);
    // A deliberate grid charge: load 7.6 kW + 0.5 kW into the pack. The grid follow leaves the
    // charge alone; the ceiling does not.
    scenario.plan = netted_plan(0.5, 7.6);
    let (outcome, sink) = scenario.run().await;
    assert_eq!(
        outcome.target_w, -100.0,
        "the charge is stopped and reversed"
    );
    assert_eq!(sink.writes(), vec![-100.0]);
    assert!(sink.writes().iter().all(|watts| *watts <= 0.0));
}

#[tokio::test]
async fn the_guard_never_asks_for_more_than_the_fleet_can_deliver() {
    // The guard asks for the whole error; the capability cap leaves the two packs' maximum.
    let mut scenario = Scenario::hold(20_000.0);
    let (outcome, sink) = scenario.run().await;
    assert_eq!(outcome.target_w, -1_600.0);
    assert_eq!(sink.writes(), vec![-800.0, -800.0]);
}

#[tokio::test]
async fn missing_or_stale_telemetry_never_guesses_a_discharge() {
    let mut scenario = Scenario::hold(7_600.0);
    scenario.meter = MeterReading::default();
    let (outcome, sink) = scenario.run().await;
    assert_eq!(outcome.target_w, 0.0, "no meter, no guard");
    assert!(sink.writes().is_empty());

    scenario.meter = MeterReading::grid(7_600.0);
    scenario.packs[0].stale = true;
    let (outcome, sink) = scenario.run().await;
    assert_eq!(
        outcome.target_w, 0.0,
        "a stale fleet cannot anchor the guard"
    );
    assert!(sink.writes().is_empty());
    // Plan staleness is a different axis (see `an_expired_plan_still_protects_the_ceiling`).
}

#[tokio::test]
async fn negative_meter_noise_around_the_release_threshold_does_not_cycle() {
    let mut scenario = Scenario::hold(6_000.0);
    scenario.limits.max_grid_import_w = 5_000.0;
    let (first, first_sink) = scenario.run().await;
    assert_eq!(first.target_w, -1_200.0);
    assert_eq!(written_w(&first_sink), -1_200.0);
    scenario.set_battery_power("SN-A", -800.0);
    scenario.set_battery_power("SN-B", -400.0);

    for noisy_grid in [4_789.0, 4_795.0, 4_801.0] {
        scenario.meter = MeterReading::grid(noisy_grid);
        let (outcome, sink) = scenario.run().await;
        assert_eq!(
            outcome.target_w, -1_200.0,
            "{noisy_grid} W must not restore the 6000 W import"
        );
        assert_eq!(written_w(&sink), -1_200.0);
        assert!(sink.writes().iter().all(|watts| *watts <= 0.0));
    }
}

#[tokio::test]
async fn a_genuine_load_drop_releases_and_restores_normal_charge() {
    let mut scenario = Scenario::hold(6_000.0);
    scenario.limits.max_grid_import_w = 5_000.0;
    let (_, _) = scenario.run().await;
    scenario.set_battery_power("SN-A", -800.0);
    scenario.set_battery_power("SN-B", -400.0);

    scenario.meter = MeterReading::grid(3_580.0);
    let (released, sink) = scenario.run().await;
    assert_eq!(released.target_w, 0.0);
    assert!(sink.writes().is_empty(), "the neutral plan is back");

    scenario.plan = netted_plan(0.2, 3.58);
    scenario.meter = MeterReading::grid(2_000.0);
    scenario.set_battery_power("SN-A", 0.0);
    let (charge, sink) = scenario.run().await;
    assert_eq!(charge.target_w, 200.0, "no zero-charge-room residue");
    assert!(
        (written_w(&sink) - 100.0).abs() < 0.5,
        "the plan's own tracking bound still applies: {} W",
        written_w(&sink)
    );
}

#[tokio::test]
async fn actuator_delay_does_not_overshoot_the_ceiling_correction() {
    let mut scenario = Scenario::hold(6_000.0);
    scenario.limits.max_grid_import_w = 5_000.0;
    let (first, _) = scenario.run().await;
    assert_eq!(first.target_w, -1_200.0);

    // Eight seconds later only 300 W arrived; the absolute correction cannot integrate to 2100 W.
    scenario.set_battery_power("SN-A", -200.0);
    scenario.set_battery_power("SN-B", -100.0);
    scenario.meter = MeterReading::grid(5_700.0);
    let (second, sink) = scenario.run().await;
    assert_eq!(second.target_w, -1_200.0);
    assert_eq!(written_w(&sink), -1_200.0);
}

#[tokio::test]
async fn an_absent_plan_still_protects_the_ceiling() {
    let mut missing = Scenario::hold(7_600.0);
    missing.no_plan = true;
    let (outcome, sink) = missing.run().await;
    assert_eq!(outcome.plan_power_w, None);
    assert_eq!(outcome.target_w, -100.0);
    assert_eq!(sink.writes(), vec![-100.0]);
    assert_eq!(
        outcome.report("SN-A").expect("writer").reason,
        Reason::GridImportCeiling
    );

    // Latches, the reserve floor and the write gate still win.
    let mut latched = Scenario::hold(7_600.0);
    latched.no_plan = true;
    latched.latches = vec![latch("SN-A")];
    let (outcome, sink) = latched.run().await;
    assert_eq!(sink.writes(), vec![-100.0]);
    assert_eq!(outcome.report("SN-A").expect("latched").intended_w, 0.0);
    let mut empty = Scenario::hold(7_600.0);
    empty.no_plan = true;
    empty.packs = vec![pack("SN-A", 10.0), pack("SN-B", 10.0)];
    let (_, sink) = empty.run().await;
    assert!(sink.writes().is_empty(), "the reserve still wins");

    let mut shadow = Scenario::hold(7_600.0);
    shadow.no_plan = true;
    shadow.mode = Authority::DryRun;
    let (outcome, sink) = shadow.run().await;
    assert_eq!(outcome.target_w, -100.0);
    assert!(sink.writes().is_empty(), "nothing is written outside auto");
}

#[tokio::test]
async fn an_expired_plan_still_protects_the_ceiling() {
    let mut expired = Scenario::hold(7_600.0);
    expired.stale = true;
    let (outcome, sink) = expired.run().await;
    assert!(outcome.stale_input);
    assert_eq!(outcome.plan_power_w, None);
    assert_eq!(outcome.target_w, -100.0);
    assert_eq!(sink.writes(), vec![-100.0]);
    assert_eq!(
        outcome.report("SN-A").expect("writer").reason,
        Reason::GridImportCeiling
    );

    let mut latched = Scenario::hold(7_600.0);
    latched.stale = true;
    latched.latches = vec![latch("SN-A")];
    let (_, sink) = latched.run().await;
    assert_eq!(sink.writes(), vec![-100.0]);
    let mut empty = Scenario::hold(7_600.0);
    empty.stale = true;
    empty.packs = vec![pack("SN-A", 10.0), pack("SN-B", 10.0)];
    let (_, sink) = empty.run().await;
    assert!(sink.writes().is_empty());

    let mut shadow = Scenario::hold(7_600.0);
    shadow.stale = true;
    shadow.mode = Authority::DryRun;
    let (outcome, sink) = shadow.run().await;
    assert_eq!(outcome.target_w, -100.0);
    assert!(sink.writes().is_empty());
}

#[tokio::test]
async fn a_stronger_planned_discharge_is_not_raised_by_the_guard() {
    let mut scenario = Scenario::hold(7_600.0);
    scenario.plan = netted_plan(-1.0, 0.0);
    let (outcome, _) = scenario.run().await;
    assert_eq!(outcome.target_w, -1_000.0, "the plan already serves more");
    assert!(
        outcome
            .packs
            .iter()
            .all(|row| row.reason != Reason::GridImportCeiling)
    );
}

#[tokio::test]
async fn an_export_limit_still_binds_when_the_guard_reverses_a_charge() {
    let mut scenario = Scenario::hold(1_000.0);
    scenario.limits.max_grid_import_w = 500.0;
    scenario.limits.grid_import_margin_w = 1_000.0; // target import 0 W
    scenario.limits.max_grid_export_w = 1_500.0;
    scenario.plan = netted_plan(1.5, 0.0);
    for slot in &mut scenario.plan.slots {
        slot.pv_kw = 1.0; // forecast a surplus while the meter still imports
    }
    let (outcome, sink) = scenario.run().await;
    assert_eq!(outcome.target_w, -1_000.0, "the charge was reversed");
    assert_eq!(
        outcome.grid_limit,
        Some(Reason::GridExportLimit),
        "the export ceiling is independent of the import guard"
    );
    assert!(
        (written_w(&sink) + 500.0).abs() < 5.0,
        "{} W",
        written_w(&sink)
    );
    assert!(sink.writes().iter().all(|watts| *watts <= 0.0));
}

#[tokio::test]
async fn the_guard_does_not_write_outside_auto() {
    let mut scenario = Scenario::hold(7_600.0);
    scenario.mode = Authority::DryRun;
    let (outcome, sink) = scenario.run().await;
    assert_eq!(
        outcome.target_w, -100.0,
        "the shadow answer still explains itself"
    );
    assert!(sink.writes().is_empty(), "no command may leave the process");
    assert_eq!(sink.holds(), 0);
}

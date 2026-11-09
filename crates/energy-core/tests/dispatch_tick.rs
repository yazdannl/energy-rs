//! One dispatcher tick: interpolation, the fleet grid check, faults and overrides.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::assert_is_empty,
    clippy::manual_assert_eq,
    clippy::assertions_on_constants
)]

mod support;

use energy_core::dispatch::{
    Dispatcher, FleetView, ManualOverride, MeterReading, OverrideAction, Overrides, TickInput,
    TickOutcome,
};
use energy_core::plan::Plan;
use energy_core::safety::{Authority, Reason, SafetyLimits};
use support::fakes::{FakeSink, NOW, pack, plan};

struct Fixture {
    plan: Plan,
    fleet: FleetView,
    meter: MeterReading,
    overrides: Overrides,
}

impl Fixture {
    /// One pack at 50 % SoC, a plan that holds `power_kw`, no overrides.
    fn one(power_kw: f64) -> Self {
        Self::with_plan(plan(NOW, power_kw, 0.96), overrides_none())
    }

    fn with_plan(plan: Plan, overrides: Overrides) -> Self {
        Self {
            plan,
            fleet: FleetView::new(vec![pack("SN-A", 50.0)]),
            meter: MeterReading::default(),
            overrides,
        }
    }

    async fn tick(
        &self,
        dispatcher: &Dispatcher,
        sink: &FakeSink,
        now: i64,
        mode: Authority,
        stale: bool,
    ) -> TickOutcome {
        let input = TickInput {
            now,
            mode,
            plan: Some(&self.plan),
            fleet: &self.fleet,
            meter: &self.meter,
            latches: &[],
            overrides: &self.overrides,
            stale,
            progress: None,
            selection: None,
        };
        dispatcher.tick(&input, sink).await
    }
}

fn overrides_none() -> Overrides {
    Overrides::none()
}

fn dispatcher(limits: SafetyLimits) -> Dispatcher {
    Dispatcher::new(limits, true, 10)
}

/// The slot boundary is a 120 s ramp, not a step from the previous slot's power.
#[tokio::test]
async fn the_setpoint_is_interpolated_over_the_first_two_minutes() {
    let mut plan = plan(NOW - 900, -0.4, 0.96);
    let current = plan
        .slots
        .iter_mut()
        .find(|slot| slot.t_start == NOW)
        .expect("current slot");
    current.battery_kw = 0.8;
    let fixture = Fixture::with_plan(plan, overrides_none());
    let sink = FakeSink::new();
    let dispatcher = dispatcher(SafetyLimits::default());

    let outcome = fixture
        .tick(&dispatcher, &sink, NOW + 60, Authority::Auto, false)
        .await;
    assert_eq!(outcome.plan_power_w.map(f64::round), Some(200.0));
    let report = outcome.report("SN-A").expect("report");
    assert!(
        (report.intended_w - 200.0).abs() < 1.0,
        "intended {}",
        report.intended_w
    );
    assert_eq!(sink.writes(), vec![200.0]);
}

/// Four individually legal packs must not exceed the site limit together.
#[tokio::test]
async fn the_fleet_grid_check_bounds_the_sum_after_allocation() {
    // Two 1.92 kWh packs: the plan is the aggregate (3.84 kWh, 1.92 kWh stored).
    let mut plan = plan(NOW, 1.6, 1.92);
    for slot in &mut plan.slots {
        slot.load_kw = 0.0;
        slot.pv_kw = 0.0;
    }
    let mut fixture = Fixture::with_plan(plan, overrides_none());
    fixture.fleet = FleetView::new(vec![pack("SN-A", 50.0), pack("SN-B", 50.0)]);
    let sink = FakeSink::new();
    let dispatcher = dispatcher(SafetyLimits {
        max_grid_import_w: 1000.0,
        ..SafetyLimits::default()
    });

    let outcome = fixture
        .tick(&dispatcher, &sink, NOW + 5, Authority::Auto, false)
        .await;
    assert_eq!(outcome.grid_limit, Some(Reason::GridImportLimit));
    let total: f64 = outcome.packs.iter().map(|pack| pack.intended_w).sum();
    assert!((total - 1000.0).abs() < 5.0, "fleet total {total}");
    let written: f64 = sink.writes().iter().sum();
    assert!((written - 1000.0).abs() < 5.0, "fleet wrote {written}");
}

/// An echo/mode fault produces exactly one real hold, then silence for the backoff.
#[tokio::test]
async fn a_fault_gets_one_fail_safe_hold_and_then_a_backoff() {
    let fixture = Fixture::one(-0.8);
    let sink = FakeSink::new();
    sink.reject_writes(true);
    let dispatcher = dispatcher(SafetyLimits::default());

    let outcome = fixture
        .tick(&dispatcher, &sink, NOW + 5, Authority::Auto, false)
        .await;
    assert_eq!(sink.writes(), vec![-800.0]);
    assert_eq!(sink.holds(), 1, "the fail-safe hold must be real");
    let report = outcome.report("SN-A").expect("report");
    assert!(report.fail_safe);
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|error| error.contains("fail-safe"))
    );

    let sent = sink.len();
    let outcome = fixture
        .tick(&dispatcher, &sink, NOW + 10, Authority::Auto, false)
        .await;
    assert_eq!(sink.len(), sent, "the backoff must send nothing");
    assert_eq!(
        outcome.report("SN-A").expect("report").reason,
        Reason::FaultBackoff
    );
}

/// A manual override wins over the plan and expires on its own.
#[tokio::test]
async fn a_manual_override_wins_and_then_expires() {
    let entry = ManualOverride::manual("SN-A", OverrideAction::Charge { watts: 500.0 }, NOW + 30);
    let fixture = Fixture::with_plan(plan(NOW, -0.8, 0.96), Overrides::new(vec![entry]));
    let sink = FakeSink::new();
    let dispatcher = dispatcher(SafetyLimits::default());

    let outcome = fixture
        .tick(&dispatcher, &sink, NOW + 5, Authority::Auto, false)
        .await;
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(report.requested_w, 500.0);
    assert_eq!(report.reason, Reason::ManualOverride);
    assert_eq!(sink.writes(), vec![500.0]);

    let outcome = fixture
        .tick(&dispatcher, &sink, NOW + 31, Authority::Auto, false)
        .await;
    let report = outcome.report("SN-A").expect("report");
    assert!(
        (report.requested_w + 800.0).abs() < 1.0,
        "expired override: {}",
        report.requested_w
    );
}

/// A stale plan holds: the previous -800 W is not repeated.
#[tokio::test]
async fn a_stale_plan_holds_instead_of_repeating_the_last_setpoint() {
    let mut reading = pack("SN-A", 50.0);
    reading.power_w = -800.0;
    let mut fixture = Fixture::one(-0.8);
    fixture.fleet = FleetView::new(vec![reading]);
    let sink = FakeSink::new();
    let dispatcher = dispatcher(SafetyLimits::default());

    let outcome = fixture
        .tick(&dispatcher, &sink, NOW + 5, Authority::Auto, true)
        .await;
    assert!(outcome.stale_input);
    assert!(sink.writes().is_empty(), "a stale plan wrote a setpoint");
    assert_eq!(sink.holds(), 1);
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(report.reason, Reason::StaleInput);
    assert_eq!(report.applied_w, 0.0);
}

/// The measured meter is corrected back to the battery-idle base before the grid bound.
#[tokio::test]
async fn the_fleet_grid_check_uses_the_corrected_measured_base() {
    let mut plan = plan(NOW, 0.8, 1.92);
    for slot in &mut plan.slots {
        slot.load_kw = 0.0;
        slot.pv_kw = 0.0;
    }
    let mut fixture = Fixture::with_plan(plan, overrides_none());
    let mut first = pack("SN-A", 50.0);
    first.power_w = 250.0;
    let mut second = pack("SN-B", 50.0);
    second.power_w = 250.0;
    fixture.fleet = FleetView::new(vec![first, second]);
    // 1200 W import while the packs already charge at 500 W -> idle base 700 W, 300 W headroom.
    fixture.meter = MeterReading::grid(1200.0);
    let sink = FakeSink::new();
    let dispatcher = dispatcher(SafetyLimits {
        max_grid_import_w: 1000.0,
        ..SafetyLimits::default()
    });

    let outcome = fixture
        .tick(&dispatcher, &sink, NOW + 5, Authority::Auto, false)
        .await;
    assert_eq!(outcome.grid_limit, Some(Reason::GridImportLimit));
    let total: f64 = outcome.packs.iter().map(|pack| pack.intended_w).sum();
    assert!((total - 300.0).abs() < 5.0, "fleet total {total}");
}

/// With a site cap configured and no usable telemetry, the fleet check holds everything.
#[tokio::test]
async fn missing_grid_telemetry_fails_the_fleet_check_closed() {
    let mut plan = plan(NOW, 0.8, 1.92);
    for slot in &mut plan.slots {
        slot.load_kw = f64::NAN;
        slot.pv_kw = f64::NAN;
    }
    let fixture = Fixture::with_plan(plan, overrides_none());
    let sink = FakeSink::new();
    let dispatcher = dispatcher(SafetyLimits {
        max_grid_import_w: 1000.0,
        ..SafetyLimits::default()
    });

    let outcome = fixture
        .tick(&dispatcher, &sink, NOW + 5, Authority::Auto, false)
        .await;
    assert_eq!(outcome.grid_limit, Some(Reason::GridTelemetryMissing));
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(report.intended_w, 0.0);
    assert_eq!(report.reason, Reason::GridTelemetryMissing);
    assert!(sink.writes().is_empty(), "nothing may be written");
}

/// A stale fleet reading makes the corrected base unusable, so the site check fails closed.
#[tokio::test]
async fn a_stale_fleet_reading_fails_the_fleet_check_closed() {
    let mut plan = plan(NOW, 0.8, 1.92);
    for slot in &mut plan.slots {
        slot.load_kw = 0.0;
        slot.pv_kw = 0.0;
    }
    let mut fixture = Fixture::with_plan(plan, overrides_none());
    let mut stale = pack("SN-A", 50.0);
    stale.stale = true;
    fixture.fleet = FleetView::new(vec![stale, pack("SN-B", 50.0)]);
    fixture.meter = MeterReading::grid(1200.0);
    let sink = FakeSink::new();
    let dispatcher = dispatcher(SafetyLimits {
        max_grid_import_w: 1000.0,
        ..SafetyLimits::default()
    });

    let outcome = fixture
        .tick(&dispatcher, &sink, NOW + 5, Authority::Auto, false)
        .await;
    assert_eq!(outcome.grid_limit, Some(Reason::GridTelemetryMissing));
    assert!(sink.writes().is_empty(), "nothing may be written");
}

/// A maintenance hold keeps the neutral setpoint and says why in the log.
#[tokio::test]
async fn a_maintenance_hold_carries_the_balance_current_note() {
    let plan = plan(NOW, 0.8, 0.96);
    let overrides = Overrides::new(vec![ManualOverride::maintenance(
        "SN-A",
        OverrideAction::Hold,
        NOW + 60,
    )]);
    let fixture = Fixture::with_plan(plan, overrides);
    let sink = FakeSink::new();
    let dispatcher = dispatcher(SafetyLimits::default());

    let outcome = fixture
        .tick(&dispatcher, &sink, NOW + 5, Authority::Auto, false)
        .await;
    let report = outcome.report("SN-A").expect("report");
    assert_eq!(report.reason, Reason::MaintenanceHold);
    assert_eq!(report.requested_w, 0.0);
    assert_eq!(sink.holds(), 1, "a neutral hold, not an acMode write");
    let note = report.note.as_deref().expect("note");
    assert!(note.contains("balance current"), "note {note}");
}

//! Watchdog bounds and progress marks inside the write/confirmation wait.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_possible_wrap
)]

mod support;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use energy_battery::{
    BatteryError, BoxFuture as IoFuture, ControlLimits, DeviceController, DeviceIo, WriteAck,
    WriteRequest, parse_report,
};
use energy_core::dispatch::{
    BatterySink, BoxFuture, CommandSink, Dispatcher, FleetView, MeterReading, Overrides,
    PackReading, Progress, SinkError, TickInput, WriteReceipt,
};
use energy_core::engine::{Engine, EngineConfig, MIN_STALL_AFTER_S, Watchdog};
use energy_core::plan::Plan;
use energy_core::safety::{Authority, SafetyLimits};
use serde_json::json;
use support::fakes::{self, FakeFleet, FakeMeter, FakeSource, NOW, pack, plan};

#[test]
fn the_stall_bound_has_a_floor_for_fast_ticks_and_keeps_three_slower_ticks() {
    // Four devices × 2 s pacing = 8 s, plus an echo round trip and slack: 15 s.
    assert_eq!(MIN_STALL_AFTER_S, 15);
    for tick in [1_u64, 5] {
        let expected = (3 * i64::try_from(tick).expect("small tick")).max(MIN_STALL_AFTER_S);
        assert_eq!(
            EngineConfig {
                tick_seconds: tick,
                ..EngineConfig::default()
            }
            .stall_after_s(),
            expected
        );
        assert_eq!(Watchdog::new(tick).stall_after_s(), expected);
    }
    assert_eq!(Watchdog::new(10).stall_after_s(), 30);
    assert_eq!(Watchdog::new(u64::MAX).stall_after_s(), i64::MAX);
}

/// A sink that can either mark a long wait or let the guard see a real gap.
#[derive(Clone, Copy)]
enum Probe {
    Marks,
    Stalls,
}

struct ProbeSink {
    script: Probe,
    engine: Mutex<Option<Weak<Engine>>>,
    commands: Mutex<Vec<String>>,
    marks: Mutex<Vec<i64>>,
}

impl ProbeSink {
    fn new(script: Probe) -> Self {
        Self {
            script,
            engine: Mutex::new(None),
            commands: Mutex::new(Vec::new()),
            marks: Mutex::new(Vec::new()),
        }
    }

    fn attach(&self, engine: &Arc<Engine>) {
        *self.engine.lock().expect("probe engine") = Some(Arc::downgrade(engine));
    }

    fn commands(&self) -> Vec<String> {
        self.commands.lock().expect("probe commands").clone()
    }

    fn marks(&self) -> Vec<i64> {
        self.marks.lock().expect("probe marks").clone()
    }

    fn reset(&self) {
        self.commands.lock().expect("probe commands").clear();
        self.marks.lock().expect("probe marks").clear();
    }

    async fn guard(&self, now: i64) -> bool {
        let upgraded = self
            .engine
            .lock()
            .expect("probe engine")
            .as_ref()
            .and_then(Weak::upgrade);
        upgraded
            .expect("attached engine")
            .supervise(now)
            .await
            .expect("guard")
    }
}

impl CommandSink for ProbeSink {
    fn write(&self, pack_id: &str, watts: f64) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        self.write_with_progress(pack_id, watts, None, NOW)
    }

    fn write_with_progress<'a>(
        &'a self,
        pack_id: &str,
        watts: f64,
        progress: Option<&'a dyn Progress>,
        _now: i64,
    ) -> BoxFuture<'a, Result<WriteReceipt, SinkError>> {
        let pack_id = pack_id.to_owned();
        Box::pin(async move {
            self.commands
                .lock()
                .expect("probe commands")
                .push(format!("write:{pack_id}"));
            match self.script {
                Probe::Marks => {
                    // A >15 s confirmation wait: the heartbeat moves with each transport step.
                    for offset in [10, 15, 21] {
                        if let Some(progress) = progress {
                            progress.mark(NOW + offset);
                        }
                        self.marks.lock().expect("probe marks").push(NOW + offset);
                    }
                    let stalled = self.guard(NOW + 21).await;
                    assert!(!stalled, "a progressing write was held by the watchdog");
                }
                Probe::Stalls => {
                    let stalled = self.guard(NOW + 22).await;
                    assert!(stalled, "a write with no progress must trip the watchdog");
                }
            }
            Ok(WriteReceipt {
                applied_w: watts,
                verified: true,
                partial: false,
                superseded: false,
                flow_measured: true,
                measured_at: Some(NOW),
                confirmation_pending: false,
            })
        })
    }

    fn hold(&self, pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        self.hold_with_progress(pack_id, None, NOW)
    }

    fn hold_with_progress<'a>(
        &'a self,
        pack_id: &str,
        _progress: Option<&'a dyn Progress>,
        _now: i64,
    ) -> BoxFuture<'a, Result<WriteReceipt, SinkError>> {
        let pack_id = pack_id.to_owned();
        Box::pin(async move {
            self.commands
                .lock()
                .expect("probe commands")
                .push(format!("hold:{pack_id}"));
            Ok(WriteReceipt {
                applied_w: 0.0,
                verified: true,
                partial: false,
                superseded: false,
                flow_measured: true,
                measured_at: Some(NOW),
                confirmation_pending: false,
            })
        })
    }
}

fn engine_with(sink: Arc<ProbeSink>, packs: Vec<PackReading>, plan: Plan) -> Arc<Engine> {
    let ids: Vec<&str> = packs.iter().map(PackReading::pack_id).collect();
    let ledger = fakes::ledger_with(&ids);
    Arc::new(Engine::new(
        EngineConfig {
            mode: Authority::Auto,
            tick_seconds: 1,
            ..EngineConfig::default()
        },
        Arc::new(FakeFleet::new(packs)),
        Arc::new(FakeMeter::default()),
        Arc::new(FakeSource::new(plan)),
        sink,
        ledger,
    ))
}

/// A tick whose write/confirmation wait keeps marking progress outlives its 15 s bound.
#[tokio::test]
async fn a_slow_write_that_marks_each_transport_step_does_not_stall() {
    let sink = Arc::new(ProbeSink::new(Probe::Marks));
    let engine = engine_with(
        Arc::clone(&sink),
        vec![pack("SN-A", 50.0)],
        plan(NOW, -0.8, 0.96),
    );
    sink.attach(&engine);
    engine.boot(NOW).await.expect("boot");
    // The first tick has no stored plan yet and holds; the next one dispatches the setpoint.
    engine.tick_once(NOW).await.expect("first tick");
    sink.reset();

    let outcome = engine.tick_once(NOW + 5).await.expect("tick");
    assert_eq!(outcome.report("SN-A").expect("report").applied_w, -800.0);
    assert_eq!(sink.marks(), vec![NOW + 10, NOW + 15, NOW + 21]);
    assert!(
        sink.commands()
            .iter()
            .all(|command| command.starts_with("write:")),
        "the watchdog held a tick that kept making progress: {:?}",
        sink.commands()
    );
}

/// A genuine gap holds both packs, fences the tick, and never retries the superseded write.
#[tokio::test]
async fn a_stalled_write_holds_and_fences_without_retrying() {
    let sink = Arc::new(ProbeSink::new(Probe::Stalls));
    let engine = engine_with(
        Arc::clone(&sink),
        vec![pack("SN-A", 50.0), pack("SN-B", 50.0)],
        plan(NOW, -1.6, 0.96),
    );
    sink.attach(&engine);
    engine.boot(NOW).await.expect("boot");
    engine.tick_once(NOW).await.expect("first tick");
    sink.reset();

    let outcome = engine.tick_once(NOW + 5).await.expect("tick");
    let commands = sink.commands();
    assert_eq!(
        commands
            .iter()
            .filter(|command| command.starts_with("write:"))
            .count(),
        1,
        "a superseded write was retried: {commands:?}"
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| command.starts_with("hold:"))
            .count(),
        2,
        "the watchdog must hold both packs: {commands:?}"
    );
    let rejected = outcome.report("SN-B").expect("report");
    assert!(!rejected.written);
    assert!(
        rejected
            .error
            .as_deref()
            .is_some_and(|error| error.contains("superseded")),
        "the second write was not fenced: {rejected:?}"
    );
}

/// A fenced command is dropped, not turned into a fail-safe hold or a retry loop.
#[tokio::test]
async fn a_fenced_write_is_not_retried() {
    struct FencedSink(AtomicUsize);
    impl CommandSink for FencedSink {
        fn write(
            &self,
            pack_id: &str,
            _watts: f64,
        ) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
            self.0.fetch_add(1, Ordering::Relaxed);
            let pack_id = pack_id.to_owned();
            Box::pin(async move { Err(SinkError::Fenced { pack_id }) })
        }
        fn hold(&self, _pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
            panic!("a fenced write must not trigger a fail-safe hold")
        }
    }

    let fixture_plan = plan(NOW, -0.8, 0.96);
    let fleet = FleetView::new(vec![pack("SN-A", 50.0)]);
    let meter = MeterReading::default();
    let overrides = Overrides::none();
    let sink = FencedSink(AtomicUsize::new(0));
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(&fixture_plan),
        fleet: &fleet,
        meter: &meter,
        latches: &[],
        overrides: &overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    let outcome = Dispatcher::new(SafetyLimits::default(), true, 10)
        .tick(&input, &sink)
        .await;
    assert_eq!(sink.0.load(Ordering::Relaxed), 1);
    let report = outcome.report("SN-A").expect("report");
    assert!(!report.written && !report.fail_safe);
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|error| error.contains("superseded"))
    );
}

struct MarkSpy(AtomicUsize);

impl Progress for MarkSpy {
    fn mark(&self, _now: i64) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

struct EchoIo {
    sn: String,
    reports: Mutex<VecDeque<energy_battery::Report>>,
}

impl DeviceIo for EchoIo {
    fn sn(&self) -> &str {
        &self.sn
    }

    fn report(&self) -> IoFuture<'_, Result<energy_battery::Report, BatteryError>> {
        let next = self
            .reports
            .lock()
            .expect("scripted reports")
            .pop_front()
            .expect("report");
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(next)
        })
    }

    fn write(&self, _request: WriteRequest) -> IoFuture<'_, Result<WriteAck, BatteryError>> {
        let sn = self.sn.clone();
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(WriteAck {
                timestamp: None,
                message_id: None,
                code: 200,
                sn: Some(sn),
            })
        })
    }
}

fn report_at(message_id: u32, output_limit: u32) -> energy_battery::Report {
    parse_report(
        json!({
            "sn": "SN-A",
            "timestamp": NOW + i64::from(message_id),
            "messageId": message_id,
            "properties": {
                "electricLevel": 50,
                "acMode": 2,
                "inputLimit": 0,
                "outputLimit": output_limit,
                "acStatus": i32::from(output_limit > 0),
                "dcStatus": i32::from(output_limit > 0) * 2,
                "outputHomePower": output_limit,
                "smartMode": 1,
                "minSoc": 5,
                "chargeMaxLimit": 800,
                "inverseMaxPower": 800
            },
            "packData": [{
                "sn": "PACK-A",
                "socLevel": 50,
                "minVol": 330,
                "maxVol": 340,
                "totalVol": 3350,
                "batcur": 10
            }]
        })
        .to_string()
        .as_bytes(),
    )
    .expect("report parses")
}

/// The real sink marks after the accepted write and during the hold's echo poll.
#[tokio::test]
async fn the_battery_sink_marks_both_the_write_and_the_echo_poll() {
    let io = EchoIo {
        sn: "SN-A".to_owned(),
        reports: Mutex::new(VecDeque::from(vec![
            report_at(1, 0),
            report_at(2, 800),
            report_at(3, 800),
            report_at(4, 0),
        ])),
    };
    let controller = Arc::new(
        DeviceController::new(io, ControlLimits::unlimited()).with_spacing(Duration::ZERO),
    );
    controller.poll().await.expect("baseline report");
    let sink = BatterySink::new(vec![Arc::clone(&controller)], Duration::MAX);
    let spy = MarkSpy(AtomicUsize::new(0));

    let write = sink
        .write_with_progress("SN-A", -800.0, Some(&spy), NOW)
        .await
        .expect("write");
    assert!(
        write.confirmation_pending,
        "flow is checked by a later read"
    );
    assert!(!write.verified);
    let after_write = spy.0.load(Ordering::Relaxed);
    assert!(after_write > 0, "the accepted write marks the watchdog");
    let hold = sink
        .hold_with_progress("SN-A", Some(&spy), NOW)
        .await
        .expect("hold");
    assert!(hold.verified, "the fresh report must echo the hold");
    assert!(
        spy.0.load(Ordering::Relaxed) > after_write,
        "the echo poll keeps marking progress"
    );
}

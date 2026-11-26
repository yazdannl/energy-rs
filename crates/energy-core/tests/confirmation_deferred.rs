//! A tick must never wait out the AC ramp: writes are accepted, flow is confirmed later.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

mod support;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use energy_battery::{
    BatteryError, BoxFuture as IoFuture, ControlLimits, DeviceController, DeviceIo, WriteAck,
    WriteRequest, parse_report,
};
use energy_core::dispatch::{
    AppliedPowerReason, BatterySink, BoxFuture, CommandSink, Dispatcher, FleetView, MeterReading,
    Overrides, Progress, SinkError, TickInput, WriteReceipt,
};
use energy_core::engine::Watchdog;
use energy_core::safety::{Authority, Reason, SafetyLimits};
use serde_json::json;
use support::fakes::{NOW, pack, plan};

const IDS: [&str; 4] = ["SN-A", "SN-B", "SN-C", "SN-D"];
const SLOW_SN: &str = "SN-A";

/// A device whose reports never echo a command: confirmation cannot arrive in this tick.
struct NeverConfirms {
    sn: &'static str,
    message_id: AtomicU32,
}

impl DeviceIo for NeverConfirms {
    fn sn(&self) -> &'static str {
        self.sn
    }

    fn report(&self) -> IoFuture<'_, Result<energy_battery::Report, BatteryError>> {
        let id = self.message_id.fetch_add(1, Ordering::Relaxed);
        let document = json!({
            "timestamp": Utc::now().timestamp(),
            "messageId": id,
            "sn": self.sn,
            "version": 3,
            "product": "solarFlow800Plus",
            "properties": {
                "acMode": 2,
                "acStatus": 0,
                "dcStatus": 0,
                "inputLimit": 0,
                "outputLimit": 0,
                "gridInputPower": 0,
                "outputHomePower": 0,
                "chargeMaxLimit": 1000,
                "inverseMaxPower": 800
            },
            "packData": []
        });
        Box::pin(async move {
            parse_report(&serde_json::to_vec(&document).expect("report json")).map_err(|error| {
                BatteryError::Payload {
                    reason: error.to_string(),
                }
            })
        })
    }

    fn write(&self, _: WriteRequest) -> IoFuture<'_, Result<WriteAck, BatteryError>> {
        let slow = self.sn == SLOW_SN;
        Box::pin(async move {
            // One device is deliberately slow; the other three must not queue behind it.
            if slow {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Ok(WriteAck {
                timestamp: None,
                message_id: None,
                code: 200,
                sn: Some(self.sn.to_owned()),
            })
        })
    }
}

/// A sink whose transport never returns: the enclosing per-pack deadline must fire.
struct HangingSink {
    marked: Mutex<Vec<String>>,
}

impl CommandSink for HangingSink {
    fn mark_uncertain(&self, pack_id: &str, phase: &str) {
        assert_eq!(
            phase, "dispatch",
            "the caller-side deadline names its phase"
        );
        self.marked.lock().unwrap().push(pack_id.to_owned());
    }

    fn write(&self, _pack_id: &str, _watts: f64) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        Box::pin(async {
            tokio::time::sleep(Duration::from_secs(120)).await;
            Ok(WriteReceipt::pending())
        })
    }

    fn hold(&self, _pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        Box::pin(async {
            tokio::time::sleep(Duration::from_secs(120)).await;
            Ok(WriteReceipt::verified(0.0))
        })
    }
}

struct Marks(Arc<Watchdog>);

impl Progress for Marks {
    fn mark(&self, now: i64) {
        self.0.mark(now);
    }
}

async fn fixture() -> (
    BatterySink<Arc<NeverConfirms>>,
    FleetView,
    energy_core::plan::Plan,
    Overrides,
) {
    let controllers: Vec<_> = IDS
        .iter()
        .map(|sn| {
            let device = Arc::new(NeverConfirms {
                sn,
                message_id: AtomicU32::new(1),
            });
            Arc::new(DeviceController::new(device, ControlLimits::unlimited()))
        })
        .collect();
    for controller in &controllers {
        controller.poll().await.expect("prime the report");
    }
    let sink = BatterySink::new(controllers, Duration::from_secs(180))
        .with_confirm_window(Duration::from_millis(100));
    let fleet = FleetView::new(IDS.iter().map(|sn| pack(sn, 50.0)).collect());
    let plan = plan(NOW, -3.2, 0.96);
    (sink, fleet, plan, Overrides::none())
}

#[tokio::test]
async fn a_never_confirming_pack_cannot_stall_the_tick_or_its_peers() {
    let (sink, fleet, plan, overrides) = fixture().await;
    let meter = MeterReading::default();
    let watchdog = Arc::new(Watchdog::new(5));
    watchdog.mark(NOW);
    let marks = Marks(Arc::clone(&watchdog));
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(&plan),
        fleet: &fleet,
        meter: &meter,
        latches: &[],
        overrides: &overrides,
        stale: false,
        progress: Some(&marks),
        selection: None,
    };

    let started = tokio::time::Instant::now();
    let outcome = Dispatcher::new(SafetyLimits::default(), true, 10)
        .tick(&input, &sink)
        .await;
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(4),
        "one slow write must not become a serial fleet wait: {elapsed:?}"
    );
    assert_eq!(outcome.writes(), IDS.len(), "all four commands were issued");
    for sn in IDS {
        let row = outcome.report(sn).expect("a row per pack");
        assert_eq!(row.reason, Reason::ConfirmationPending, "{sn}: {row:?}");
        assert!(!row.verified && !row.flow_measured, "{sn}: {row:?}");
        assert_eq!(
            row.applied_w_reason,
            Some(AppliedPowerReason::ConfirmationPending),
            "{sn}: {row:?}"
        );
        assert_eq!(row.applied_w, 0.0);
    }

    // This is the same heartbeat the health route reads as `lastTick`: a completed tick advances
    // it, so `/api/v1/health` cannot report `tick_stalled` for this fleet.
    watchdog.mark(NOW + elapsed.as_secs() as i64);
    assert!(watchdog.last_progress().is_some());
    assert!(
        !watchdog.stalled(NOW + elapsed.as_secs() as i64),
        "a bounded tick keeps the watchdog healthy"
    );
}

/// A changed plan must not be reported as written while the previous command still owns the pack.
#[tokio::test]
async fn a_changed_setpoint_is_blocked_not_reported_as_pending() {
    let (sink, _, _, _) = fixture().await;
    let first = sink.write(SLOW_SN, -320.0).await.expect("first write");
    assert!(first.confirmation_pending, "first write is the pending one");
    let second = sink.write(SLOW_SN, -640.0).await.expect("second attempt");
    assert!(
        second.blocked_by_pending,
        "replacement must be distinguished: {second:?}"
    );
    assert!(!second.verified && !second.confirmation_pending);
}

/// A sink that never returns is an uncertain command, not an accepted write with a pending receipt.
#[tokio::test(start_paused = true)]
async fn a_per_pack_deadline_is_reported_as_uncertain_not_accepted() {
    let (_, fleet, plan, overrides) = fixture().await;
    let meter = MeterReading::default();
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(&plan),
        fleet: &fleet,
        meter: &meter,
        latches: &[],
        overrides: &overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    let dispatcher = Dispatcher::new(SafetyLimits::default(), true, 10);
    let started = tokio::time::Instant::now();
    let sink = HangingSink {
        marked: Mutex::new(Vec::new()),
    };
    let outcome = dispatcher.tick(&input, &sink).await;
    assert_eq!(
        started.elapsed(),
        energy_battery::control::PACK_IO_BUDGET,
        "the published budget must be the enforced one"
    );
    let row = outcome.report(SLOW_SN).expect("a row per pack");
    assert_eq!(row.reason, Reason::SendTimedOut, "{row:?}");
    assert!(!row.written, "no accepted write claimed: {row:?}");
    assert_eq!(row.applied_w_reason, Some(AppliedPowerReason::ReadFailed));
    assert!(
        row.error
            .as_deref()
            .is_some_and(|error| error.contains("uncertain")),
        "the phase must be named: {row:?}"
    );
    assert!(
        row.error
            .as_deref()
            .is_some_and(|error| error.contains("no recovery attempted")),
        "no recovery was attempted inside a cancelled send: {row:?}"
    );
    assert!(
        sink.marked.lock().unwrap().contains(&SLOW_SN.to_owned()),
        "the caller-side deadline must mark every cancelled pack uncertain"
    );
    assert!(
        dispatcher.faults().blocking(SLOW_SN, NOW),
        "the uncertain command must arm backoff"
    );
}

/// The blocked flag must survive into the published row, not fall back to the plan reason.
#[tokio::test]
async fn a_blocked_receipt_names_the_blocked_command_in_the_row() {
    struct BlockedSink;

    impl CommandSink for BlockedSink {
        fn write(
            &self,
            _pack_id: &str,
            _watts: f64,
        ) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
            Box::pin(async { Ok(WriteReceipt::blocked_by_pending()) })
        }

        fn hold(&self, _pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
            Box::pin(async { Ok(WriteReceipt::verified(0.0)) })
        }
    }

    let (_, fleet, plan, overrides) = fixture().await;
    let meter = MeterReading::default();
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(&plan),
        fleet: &fleet,
        meter: &meter,
        latches: &[],
        overrides: &overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    let outcome = Dispatcher::new(SafetyLimits::default(), true, 10)
        .tick(&input, &BlockedSink)
        .await;
    let row = outcome.report(SLOW_SN).expect("a row per pack");
    assert_eq!(row.reason, Reason::PreviousCommandPending, "{row:?}");
    assert!(!row.written && !row.verified, "{row:?}");
    assert!(
        row.error
            .as_deref()
            .is_some_and(|error| error.contains("previous command")),
        "{row:?}"
    );
}

/// If fail-safe recovery began before the outer deadline cancelled it, the row must not claim that
/// nothing was attempted; the hold's outcome is genuinely unknown.
#[tokio::test(start_paused = true)]
async fn a_deadline_during_recovery_reports_an_unknown_outcome() {
    struct RecoveryHangingSink;

    impl CommandSink for RecoveryHangingSink {
        fn write(
            &self,
            pack_id: &str,
            _watts: f64,
        ) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
            let pack_id = pack_id.to_owned();
            Box::pin(async move {
                Err(SinkError::DeviceUnreachable {
                    pack_id,
                    reason: "scripted write failure".to_owned(),
                })
            })
        }

        fn hold(&self, _pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(120)).await;
                Ok(WriteReceipt::verified(0.0))
            })
        }
    }

    let (_, fleet, plan, overrides) = fixture().await;
    let meter = MeterReading::default();
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(&plan),
        fleet: &fleet,
        meter: &meter,
        latches: &[],
        overrides: &overrides,
        stale: false,
        progress: None,
        selection: None,
    };
    let outcome = Dispatcher::new(SafetyLimits::default(), true, 10)
        .tick(&input, &RecoveryHangingSink)
        .await;
    let row = outcome.report(SLOW_SN).expect("a row per pack");
    assert_eq!(row.reason, Reason::SendTimedOut, "{row:?}");
    assert!(
        row.error
            .as_deref()
            .is_some_and(|error| error.contains("recovery started, outcome unknown")),
        "the recovery state must be honest: {row:?}"
    );
}

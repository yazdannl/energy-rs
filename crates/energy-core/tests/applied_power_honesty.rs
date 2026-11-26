//! Applied power must come from a fresh measurement, never from an echo or a stale sample.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

mod support;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::Utc;
use energy_battery::{
    BatteryError, BoxFuture as IoFuture, ControlLimits, DeviceController, DeviceIo, WriteAck,
    WriteRequest, parse_report,
};
use energy_core::dispatch::{
    AppliedPowerReason, BatterySink, Dispatcher, FleetView, MeterReading, Overrides, PackReport,
    TickInput,
};
use energy_core::safety::{Authority, Reason, SafetyLimits};
use serde_json::json;
use support::fakes::{NOW, pack, plan};

const SN: &str = "SN-A";
const FRESHNESS: Duration = Duration::from_secs(30);
const STALE_AGE_S: i64 = 60;

/// A report echo: input limit, output limit, grid input, home output.
type Echo = (i64, i64, i64, i64);

/// Which dispatcher command this device should receive.
#[derive(Clone, Copy)]
enum Case {
    /// A full 400 W charge: fresh flow is 400 W.
    Charge,
    /// A 400 W charge echoed at 396 W with 20 W flowing: a genuine partial observation.
    Partial,
    /// A 0 W hold: fresh power is genuinely idle.
    Hold,
}

struct ScriptedDevice {
    message_id: Mutex<u32>,
    echo: Mutex<Echo>,
    age_s: i64,
    partial: bool,
    fail_reads: AtomicBool,
    refuse_write: AtomicBool,
}

impl ScriptedDevice {
    fn new(age_s: i64) -> Self {
        Self {
            message_id: Mutex::new(0),
            echo: Mutex::new((0, 0, 0, 0)),
            age_s,
            partial: false,
            fail_reads: AtomicBool::new(false),
            refuse_write: AtomicBool::new(false),
        }
    }

    fn partial(mut self) -> Self {
        self.partial = true;
        self
    }

    fn fail_reads(&self) {
        self.fail_reads.store(true, Ordering::Relaxed);
    }

    fn refuse_write(&self) {
        self.refuse_write.store(true, Ordering::Relaxed);
    }
}

impl DeviceIo for ScriptedDevice {
    fn sn(&self) -> &'static str {
        SN
    }

    fn report(&self) -> IoFuture<'_, Result<energy_battery::Report, BatteryError>> {
        if self.fail_reads.load(Ordering::Relaxed) {
            return Box::pin(async {
                Err(BatteryError::Transport {
                    host: "device".to_owned(),
                    port: 80,
                    reason: "device report request failed".to_owned(),
                })
            });
        }
        let mut id = self.message_id.lock().expect("message id");
        *id += 1;
        let (input_limit, output_limit, grid_w, home_w) = *self.echo.lock().expect("echo");
        let document = json!({
            "timestamp": Utc::now().timestamp() - self.age_s,
            "messageId": *id,
            "sn": SN,
            "version": 3,
            "product": "solarFlow800Plus",
            "properties": {
                "acMode": 1,
                "acStatus": 2,
                "dcStatus": 1,
                "inputLimit": input_limit,
                "outputLimit": output_limit,
                "gridInputPower": grid_w,
                "outputHomePower": home_w,
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

    fn write(&self, request: WriteRequest) -> IoFuture<'_, Result<WriteAck, BatteryError>> {
        if self.refuse_write.swap(false, Ordering::Relaxed) {
            return Box::pin(async {
                Err(BatteryError::Rejected {
                    code: 400,
                    message: "device refused the write".to_owned(),
                })
            });
        }
        let properties = request.properties();
        let input = i64::from(properties.input_limit().unwrap_or(0));
        let output = i64::from(properties.output_limit().unwrap_or(0));
        let echo = if input == 0 && output == 0 {
            (0, 0, 0, 0)
        } else if self.partial {
            (input - 4, output, 20, 0)
        } else if input > 0 {
            (input, output, 400, 0)
        } else {
            (input, output, 0, 300)
        };
        *self.echo.lock().expect("echo") = echo;
        Box::pin(async {
            Ok(WriteAck {
                timestamp: None,
                message_id: None,
                code: 200,
                sn: Some(SN.to_owned()),
            })
        })
    }
}

async fn tick_case(
    device: &std::sync::Arc<ScriptedDevice>,
    fail_after_prime: bool,
    case: Case,
) -> (PackReport, f64) {
    let controller = std::sync::Arc::new(DeviceController::new(
        std::sync::Arc::clone(device),
        ControlLimits::unlimited(),
    ));
    controller.poll().await.expect("prime the report");
    if fail_after_prime {
        device.fail_reads();
    }
    let sink = std::sync::Arc::new(
        BatterySink::new(vec![std::sync::Arc::clone(&controller)], FRESHNESS)
            .with_confirm_window(Duration::from_millis(100)),
    );
    let power_kw = match case {
        Case::Hold => 0.0,
        Case::Charge | Case::Partial => 0.4,
    };
    let plan = plan(NOW, power_kw, 0.96);
    let fleet = FleetView::new(vec![pack(SN, 50.0)]);
    let meter = MeterReading {
        grid_w: Some(403.792),
        ..MeterReading::default()
    };
    let overrides = Overrides::none();
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
    let outcome = dispatcher.tick(&input, sink.as_ref()).await;
    (
        outcome.report(SN).expect("pack row").clone(),
        meter.grid_w.expect("grid power"),
    )
}

fn assert_unmeasured(report: &PackReport, reason: AppliedPowerReason) {
    assert_eq!(report.applied_w, 0.0);
    assert!(!report.flow_measured);
    assert_eq!(report.applied_w_reason, Some(reason));
}

#[tokio::test]
async fn a_stale_nonzero_sample_has_null_applied_w_and_sample_stale_reason() {
    let device = std::sync::Arc::new(ScriptedDevice::new(STALE_AGE_S));
    let (report, independent_grid_w) = tick_case(&device, false, Case::Charge).await;

    assert!(
        independent_grid_w.abs() > 1.0,
        "an independent meter sees real power"
    );
    assert_unmeasured(&report, AppliedPowerReason::ConfirmationPending);
    assert_eq!(report.reason, Reason::ConfirmationPending);
    assert!(
        !report.verified,
        "flow is no longer awaited inside the tick"
    );
}

#[tokio::test]
async fn a_failed_read_has_null_applied_w_and_read_failed_reason() {
    let device = std::sync::Arc::new(ScriptedDevice::new(0));
    device.refuse_write();
    let (report, _) = tick_case(&device, true, Case::Charge).await;

    assert_unmeasured(&report, AppliedPowerReason::ReadFailed);
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|error| error.contains("request failed")),
        "the row must name the failed read: {:?}",
        report.error
    );
}

#[tokio::test]
async fn a_partial_command_is_reported_as_pending_not_measured_zero() {
    let device = std::sync::Arc::new(ScriptedDevice::new(STALE_AGE_S).partial());
    let (report, independent_grid_w) = tick_case(&device, false, Case::Partial).await;

    assert!(independent_grid_w.abs() > 1.0);
    assert_unmeasured(&report, AppliedPowerReason::ConfirmationPending);
    assert_eq!(report.reason, Reason::ConfirmationPending);
}

#[tokio::test]
async fn a_stale_hold_has_null_applied_w_and_sample_stale_reason() {
    let device = std::sync::Arc::new(ScriptedDevice::new(STALE_AGE_S));
    let (report, _) = tick_case(&device, false, Case::Hold).await;

    assert_unmeasured(&report, AppliedPowerReason::ConfirmationPending);
}

#[tokio::test]
async fn a_measured_fail_safe_hold_has_no_applied_power_reason() {
    let device = std::sync::Arc::new(ScriptedDevice::new(0));
    device.refuse_write();
    let (report, _) = tick_case(&device, false, Case::Charge).await;

    assert!(report.fail_safe);
    assert!(report.flow_measured);
    assert_eq!(report.applied_w_reason, None);
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|error| error.contains("fail-safe hold")),
        "the earlier refusal belongs in the error: {:?}",
        report.error
    );
}

#[tokio::test]
async fn a_stale_fail_safe_hold_has_null_applied_w_and_sample_stale_reason() {
    let device = std::sync::Arc::new(ScriptedDevice::new(STALE_AGE_S));
    device.refuse_write();
    let (report, _) = tick_case(&device, false, Case::Charge).await;

    assert_unmeasured(&report, AppliedPowerReason::SampleStale);
    assert!(report.fail_safe);
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|error| error.contains("fail-safe hold")),
        "the original refusal stays in the error: {:?}",
        report.error
    );
}

//! Applied power must come from a fresh measurement, never from an echo or a stale sample.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_wrap
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
    AppliedPowerReason, BatterySink, Dispatcher, FleetView, MeterReading, Overrides, TickInput,
};
use energy_core::safety::{Authority, SafetyLimits};
use serde_json::json;
use support::fakes::{NOW, pack, plan};

const SN: &str = "SN-A";

struct ScriptedDevice {
    message_id: Mutex<u32>,
    age_s: i64,
    fail_reads: AtomicBool,
}

impl ScriptedDevice {
    fn new(age_s: i64) -> Self {
        Self {
            message_id: Mutex::new(0),
            age_s,
            fail_reads: AtomicBool::new(false),
        }
    }

    fn fail_reads(&self) {
        self.fail_reads.store(true, Ordering::Relaxed);
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
                "inputLimit": 400,
                "outputLimit": 0,
                "gridInputPower": 400,
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

async fn tick(
    device: &std::sync::Arc<ScriptedDevice>,
    freshness: Duration,
    fail_after_prime: bool,
) -> (energy_core::dispatch::PackReport, f64) {
    let controller = std::sync::Arc::new(DeviceController::new(
        std::sync::Arc::clone(device),
        ControlLimits::unlimited(),
    ));
    controller.poll().await.expect("prime the report");
    if fail_after_prime {
        device.fail_reads();
    }
    let sink = std::sync::Arc::new(
        BatterySink::new(vec![std::sync::Arc::clone(&controller)], freshness)
            .with_confirm_window(Duration::from_millis(100)),
    );
    let plan = plan(NOW, 0.4, 0.96);
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

#[tokio::test]
async fn a_stale_nonzero_sample_has_null_applied_w_and_sample_stale_reason() {
    let device = std::sync::Arc::new(ScriptedDevice::new(60));
    let (report, independent_grid_w) = tick(&device, Duration::from_secs(30), false).await;

    assert!(
        independent_grid_w.abs() > 1.0,
        "an independent meter sees real power"
    );
    assert_eq!(report.applied_w, 0.0);
    assert!(!report.flow_measured);
    assert_eq!(
        report.applied_w_reason,
        Some(AppliedPowerReason::SampleStale)
    );
    assert!(
        report.verified,
        "the echo landed; only the sample was stale"
    );
}

#[tokio::test]
async fn a_failed_read_has_null_applied_w_and_read_failed_reason() {
    let device = std::sync::Arc::new(ScriptedDevice::new(0));
    let (report, _) = tick(&device, Duration::from_secs(30), true).await;

    assert_eq!(report.applied_w, 0.0);
    assert!(!report.flow_measured);
    assert_eq!(
        report.applied_w_reason,
        Some(AppliedPowerReason::ReadFailed)
    );
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|error| error.contains("request failed")),
        "the row must name the failed read: {:?}",
        report.error
    );
}

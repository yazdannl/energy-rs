//! Engine + real DeviceController/BatterySink proof across the flow-recovery deadline.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::unreadable_literal
)]

mod support;

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use energy_battery::{
    BatteryError, BoxFuture as IoFuture, ControlLimits, DeviceController, DeviceIo, WriteAck,
    WriteRequest, parse_report,
};
use energy_core::dispatch::{BatterySink, PackFlow};
use energy_core::engine::{Engine, EngineConfig, LiveLatch};
use energy_core::plan::Plan;
use energy_core::safety::Authority;
use serde_json::json;
use support::fakes::{self, FakeFleet, FakeMeter, FakeSource, NOW, pack, plan};

const SN: &str = "SN-A";

/// Real controller transport with a stale echo (inputLimit 0) but real 1000 W AC flow. Every
/// post-write sample's device clock lags `accepted_at` by exactly 1..6 s: the 04:45Z shape.
struct LaggingFlow {
    message_id: AtomicU32,
    accepted_at: Mutex<Option<i64>>,
    baseline: i64,
    writes: Mutex<Vec<(Option<i32>, Option<i32>)>>,
    holds: AtomicUsize,
}

impl LaggingFlow {
    fn new() -> Self {
        Self {
            message_id: AtomicU32::new(1),
            accepted_at: Mutex::new(None),
            baseline: Utc::now().timestamp() - 20,
            writes: Mutex::new(Vec::new()),
            holds: AtomicUsize::new(0),
        }
    }

    fn set_accepted_at(&self, at: i64) {
        *self.accepted_at.lock().unwrap() = Some(at);
    }

    fn sample_timestamp(&self, id: u32) -> i64 {
        match *self.accepted_at.lock().unwrap() {
            Some(at) => at - 1 - i64::from(id % 6),
            None => self.baseline,
        }
    }

    fn writes(&self) -> Vec<(Option<i32>, Option<i32>)> {
        self.writes.lock().unwrap().clone()
    }
}

impl DeviceIo for LaggingFlow {
    fn sn(&self) -> &'static str {
        SN
    }

    fn report(&self) -> IoFuture<'_, Result<energy_battery::Report, BatteryError>> {
        let id = self.message_id.fetch_add(1, Ordering::Relaxed);
        let ts = self.sample_timestamp(id);
        let flowing = self.accepted_at.lock().unwrap().is_some();
        let document = json!({
            "timestamp": ts,
            "messageId": id,
            "sn": SN,
            "version": 3,
            "product": "solarFlow800Plus",
            "properties": {
                "ts": ts,
                "acMode": 1,
                "acStatus": if flowing { 2 } else { 0 },
                "dcStatus": i32::from(flowing),
                "inputLimit": 0,
                "outputLimit": 0,
                "gridInputPower": if flowing { 1000 } else { 0 },
                "chargeMaxLimit": 1000,
                "inverseMaxPower": 1000
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
        let properties = request.properties();
        self.writes
            .lock()
            .unwrap()
            .push((properties.ac_mode(), properties.input_limit()));
        if properties.ac_mode().is_none() {
            self.holds.fetch_add(1, Ordering::Relaxed);
        }
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

fn charging_reading() -> energy_core::dispatch::PackReading {
    let mut reading = pack(SN, 50.0);
    reading.spec.max_charge_w = 1_000.0;
    reading.flow = Some(PackFlow {
        grid_input_w: 1_000,
        output_pack_w: 0,
        output_home_w: 0,
        ac_status: Some(2),
        dc_status: Some(1),
    });
    reading
}

fn assert_charge_intact(
    device: &LaggingFlow,
    controller: &DeviceController<Arc<LaggingFlow>>,
    engine: &Engine,
    writes_from: usize,
    holds_from: usize,
) {
    let writes = device.writes();
    assert!(
        writes
            .iter()
            .any(|(mode, input)| *mode == Some(1) && *input == Some(1_000)),
        "the real sink wrote a 1000 W charge: {writes:?}"
    );
    assert!(
        writes[writes_from..]
            .iter()
            .all(|(mode, input)| *mode == Some(1) && *input == Some(1_000)),
        "no zero or hold command after the charge started: {writes:?}"
    );
    assert_eq!(
        device.holds.load(Ordering::Relaxed),
        holds_from,
        "no hold was written after start"
    );
    assert!(
        controller.active_fault().is_none(),
        "no active controller fault"
    );
    assert_eq!(
        LiveLatch::load(engine.ledger())
            .expect("durable latch")
            .get(SN)
            .expect("charge still standing")
            .power_w,
        1_000.0
    );
}

async fn build_engine() -> (
    Arc<LaggingFlow>,
    Arc<DeviceController<Arc<LaggingFlow>>>,
    Arc<Engine>,
    Plan,
) {
    let device = Arc::new(LaggingFlow::new());
    let controller = Arc::new(
        DeviceController::new(Arc::clone(&device), ControlLimits::unlimited())
            .with_spacing(Duration::ZERO),
    );
    controller.poll().await.expect("prime real controller");
    let sink = Arc::new(
        BatterySink::new(vec![Arc::clone(&controller)], Duration::MAX)
            .with_confirm_window(Duration::from_millis(100)),
    );
    let planned = plan(NOW, 1.0, 1.0);
    let engine = Arc::new(Engine::new(
        EngineConfig {
            mode: Authority::Auto,
            ..EngineConfig::default()
        },
        Arc::new(FakeFleet::new(vec![charging_reading()])),
        Arc::new(FakeMeter::default()),
        Arc::new(FakeSource::new(planned.clone())),
        sink,
        fakes::ledger_with(&[SN]),
    ));
    engine.boot(NOW).await.expect("boot");
    engine.tick_once(NOW).await.expect("prime the plan");
    engine.tick_once(NOW).await.expect("first charge tick");
    (device, controller, engine, planned)
}

#[tokio::test(start_paused = true)]
async fn an_engine_charge_with_skewed_flow_stays_active_past_the_recovery_window() {
    let (device, controller, engine, _planned) = build_engine().await;
    let accepted_at = controller
        .pending()
        .unwrap_or_else(|| panic!("controller owns charge; writes={:?}", device.writes()))
        .accepted_at;
    device.set_accepted_at(accepted_at);
    let writes_from = device.writes().len();
    let holds_from = device.holds.load(Ordering::Relaxed);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    assert!(
        device.baseline < accepted_at - 6,
        "lag stays after baseline"
    );

    for tick in 1..=48 {
        let sample_at = device.sample_timestamp(tick + 1);
        assert!(sample_at < accepted_at, "sample {sample_at} predates write");
        tokio::time::advance(Duration::from_secs(1)).await;
        let outcome = engine
            .tick_once(NOW + i64::from(tick))
            .await
            .unwrap_or_else(|error| panic!("tick {tick}: {error}"));
        let seen = controller
            .poll()
            .await
            .expect("fold real controller report");
        assert!(
            seen.report.timestamp < accepted_at,
            "every consumed sample is pre-write"
        );
        assert!(seen.fault.is_none(), "no control fault: {seen:?}");
        assert!(controller.active_fault().is_none(), "no active hold fault");
        assert!(
            outcome.report(SN).is_some(),
            "the engine dispatched this tick"
        );
        assert!(
            !engine
                .dispatcher()
                .faults()
                .blocking(SN, NOW + i64::from(tick)),
            "no backoff at actual tick time {tick}"
        );
    }
    assert!(
        tokio::time::Instant::now() >= deadline,
        "the test crossed the real recovery deadline"
    );
    assert_charge_intact(&device, &controller, &engine, writes_from, holds_from);
}

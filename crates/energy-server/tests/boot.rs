//! End-to-end boot: temporary `/data`, an in-process Home Assistant, four fake devices.
//!
//! Hermetic by construction: the HA URL and every battery endpoint point at `127.0.0.1` ephemeral
//! ports, mDNS is off, and the price book comes from the fake HA's tariff sensor. The test boots the
//! real binary path (`bootstrap::boot`), queries the real router, sends a real mode PATCH and takes
//! the real shutdown path.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    // One assertion block per route reads better than four one-call helpers.
    clippy::cognitive_complexity
)]

mod support;

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use support::fake_ha::{FakeHa, TOKEN};
use tempfile::TempDir;
use tower::ServiceExt;

use energy_battery::fake::{FakeDevice, FakePack, FakeState};
use energy_core::safety::Authority;
use energy_price::{PriceBook, PriceRepository, Slot, SlotSource, SqlitePriceRepository};
use energy_server::bootstrap::{boot, options::Options};

const SERIALS: [&str; 4] = ["AB2000-ONE", "AB2000-TWO", "AB2000-THREE", "AB2000-FOUR"];

/// A process-wide log sink, installed once; every boot in this binary writes into it.
fn logs() -> Arc<Mutex<Vec<u8>>> {
    static SINK: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    SINK.get_or_init(|| {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = LogWriter(Arc::clone(&buffer));
        let _ = tracing_subscriber::fmt()
            .with_writer(writer)
            .with_ansi(false)
            .try_init();
        buffer
    })
    .clone()
}

#[derive(Clone)]
struct LogWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut buffer) = self.0.lock() {
            buffer.extend_from_slice(bytes);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

async fn devices() -> Vec<FakeDevice> {
    let mut devices = Vec::new();
    for (index, serial) in SERIALS.iter().enumerate() {
        let state = FakeState::new(*serial)
            .with_soc(50.0 + index as f64 * 5.0)
            .with_packs(vec![FakePack::ab2000(*serial)]);
        devices.push(
            FakeDevice::spawn(state)
                .await
                .expect("the fake device starts"),
        );
    }
    devices
}

/// A known, gap-free price book: three days of quarter-hour slots around now.
///
/// The same store the runtime opens, seeded before boot, so the plan never depends on what time of
/// day the fake tariff sensor's two-day forecast happens to cover.
fn seed_prices(dir: &TempDir) {
    let now = chrono::Utc::now().timestamp();
    let first = now - now.rem_euclid(900);
    let mut slots = Vec::new();
    for index in 0..288 {
        let t_start = first + i64::from(index) * 900;
        let buy = 0.20 + f64::from(index % 24) * 0.005;
        slots.push(Slot::new(
            t_start,
            buy,
            (buy - 0.02) / 1.21 - 0.09161,
            SlotSource::ZonneplanQuarterHourly,
        ));
    }
    let book = PriceBook::new(slots).expect("a contiguous book");
    let store = SqlitePriceRepository::open(dir.path().join("energy_rs.db"))
        .expect("the price store opens");
    store.save_publication(&book).expect("the book is stored");
}

fn options(dir: &TempDir, ha: &FakeHa, devices: &[FakeDevice], mode: Authority) -> Options {
    let mut options = Options::at(dir.path())
        .expect("the options load")
        .with_direct_ha(ha.base_url(), TOKEN)
        .with_mdns(false)
        // Discovery stays off: the real LAN may well hold packs, and a boot test must not
        // depend on what else answers on the network it happens to run in.
        .with_scan(false)
        .with_port(0)
        .with_tick_seconds(1)
        .with_mode(mode);
    for device in devices {
        options = options.with_manual_battery(device.host(), device.serial(), device.port());
    }
    options
}

async fn call(
    router: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(value) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = router
        .clone()
        .oneshot(request.body(body).expect("a request"))
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// Wait for the engine's first published tick.
async fn wait_for_tick(
    service: &energy_server::Service,
    timeout: Duration,
) -> energy_core::stream::EngineSnapshot {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let snapshot = service.snapshot();
        if snapshot.last_tick.is_some() {
            return snapshot;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the engine published no tick within {timeout:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn boot_dry_run(
    dir: &TempDir,
    ha: &FakeHa,
    devices: &[FakeDevice],
) -> energy_server::bootstrap::Booted {
    seed_prices(dir);
    boot(options(dir, ha, devices, Authority::DryRun))
        .await
        .expect("the process boots")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boots_serves_and_publishes_a_snapshot() {
    let logs = logs();
    let ha = FakeHa::spawn().await;
    let devices = devices().await;
    let dir = tempfile::tempdir().expect("a temp dir");
    let booted = boot_dry_run(&dir, &ha, &devices).await;

    let snapshot = wait_for_tick(&booted.service, Duration::from_secs(20)).await;
    assert_eq!(snapshot.mode, Authority::DryRun);
    assert!(snapshot.site.configured);
    assert_eq!(snapshot.packs.len(), 4);
    assert!(snapshot.meter.is_some_and(|meter| meter.grid_w.is_finite()));
    // The plan source read HA statistics, the solar forecast and the price book and solved.
    let plan = snapshot.plan.expect("the first tick produced a plan");
    assert!(plan.plan_id > 0, "the plan was not persisted");
    // A smoke test, not a shape test: with net metering on the solver trades through the horizon,
    // so more constraints bind than a pure self-consumption plan would.
    assert!(
        plan.binding_reasons.len() < 16,
        "implausible binding reasons: {:?}",
        plan.binding_reasons
    );
    // The boot learned a load profile from the fake dashboard, so the plan is not degraded by the
    // residential fallback any more.
    let solved = booted.engine.current_plan().expect("plan").expect("plan");
    assert!(
        solved.degraded.is_none(),
        "the learned profile should not degrade the plan: {:?}; faults {:?}",
        solved.degraded,
        snapshot.faults
    );
    assert!(solved.slots.len() >= 4);

    read_routes(&booted).await;
    booted.shutdown().await.expect("a clean stop");

    let text = String::from_utf8_lossy(
        &logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
    .to_string();
    assert!(
        text.contains("energy-rs ready"),
        "the startup summary is missing:\n{text}"
    );
    assert!(!text.contains(TOKEN), "the HA token leaked into the log");
}

/// The read surface answers with the discovered site, the four packs and the embedded UI.
async fn read_routes(booted: &energy_server::bootstrap::Booted) {
    let router = booted.router();
    let (status, health) = call(&router, Method::GET, "/api/v1/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["status"], "ok");
    assert_eq!(health["mode"], "dry_run");
    assert!(health["lastTickAt"].is_string());

    let (status, site) = call(&router, Method::GET, "/api/v1/site", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(site["configured"], true);
    assert_eq!(site["packs"].as_array().map(Vec::len), Some(4));
    assert_eq!(site["capabilities"][0], "battery_control");

    let (status, packs) = call(&router, Method::GET, "/api/v1/packs", None).await;
    assert_eq!(status, StatusCode::OK);
    let packs = packs["packs"].as_array().cloned().unwrap_or_default();
    assert_eq!(packs.len(), 4);
    assert!(packs.iter().all(|pack| pack["state"]["socPct"].is_number()));

    let (status, _) = call(&router, Method::GET, "/", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_mode_patch_reaches_the_engine() {
    let ha = FakeHa::spawn().await;
    let devices = devices().await;
    let dir = tempfile::tempdir().expect("a temp dir");
    let booted = boot_dry_run(&dir, &ha, &devices).await;
    wait_for_tick(&booted.service, Duration::from_secs(20)).await;

    let router = booted.router();
    let (status, body) = call(
        &router,
        Method::PATCH,
        "/api/v1/mode",
        Some(json!({"mode": "auto"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["engineNotified"], true);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while booted.engine.mode().expect("mode") != Authority::Auto {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the mode never reached the engine"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    wait_for_mode(&booted.service, Authority::Auto, Duration::from_secs(5)).await;
    booted.shutdown().await.expect("a clean stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dry_run_applies_nothing() {
    let ha = FakeHa::spawn().await;
    let devices = devices().await;
    let dir = tempfile::tempdir().expect("a temp dir");
    let booted = boot_dry_run(&dir, &ha, &devices).await;
    wait_for_tick(&booted.service, Duration::from_secs(20)).await;
    tokio::time::sleep(Duration::from_millis(2_000)).await;
    for device in &devices {
        assert_eq!(
            device.state().unwrap().write_count(),
            0,
            "dry_run wrote to the hardware"
        );
    }
    booted.shutdown().await.expect("a clean stop");
    for device in &devices {
        assert_eq!(
            device.state().unwrap().write_count(),
            0,
            "the dry-run stop wrote to the hardware"
        );
    }
}

async fn wait_for_mode(service: &energy_server::Service, mode: Authority, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    while service.snapshot().mode != mode {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the snapshot never showed {mode:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stop_holds_every_pack_in_auto_and_only_then() {
    let ha = FakeHa::spawn().await;
    let devices = devices().await;
    let dir = tempfile::tempdir().expect("a temp dir");
    seed_prices(&dir);
    let booted = boot(options(&dir, &ha, &devices, Authority::Auto))
        .await
        .expect("the process boots");
    let snapshot = wait_for_tick(&booted.service, Duration::from_secs(20)).await;
    assert_eq!(snapshot.mode, Authority::Auto);

    booted.shutdown().await.expect("a clean stop");

    for device in &devices {
        let state = device.state().unwrap();
        assert!(
            state.write_count() >= 1,
            "{} was never neutralised on the way out",
            device.serial()
        );
        let last = state.last_body().cloned().unwrap_or(Value::Null);
        assert_eq!(
            last["properties"]["inputLimit"], 0,
            "the exit hold is not neutral"
        );
        assert_eq!(
            last["properties"]["outputLimit"], 0,
            "the exit hold is not neutral"
        );
    }
}

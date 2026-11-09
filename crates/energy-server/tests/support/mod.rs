//! Shared fixtures: a temporary ledger seeded with a plan, dispatch rows, savings, settings and
//! prices, a service over it, and small HTTP helpers.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

/// The in-process Home Assistant used by the boot test.
pub mod fake_ha;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use chrono::{Duration as ChronoDuration, Utc};
use chrono_tz::Tz;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

use energy_battery::{CellLatch, LearningProfile};
use energy_core::safety::Authority;
use energy_core::stream::{
    EngineSnapshot, FaultSnapshot, ForecastSnapshot, MeterSnapshot, PackSnapshot, PlanSnapshot,
    SiteSnapshot, SiteSource,
};
use energy_ledger::{
    DispatchEntry, Ledger, LedgerConfig, Pack, Plan, PlanSlot, Sample, SavingsDaily,
};
use energy_price::{
    MemoryPriceRepository, PriceBook, PriceRepository, Settlement, Slot, SlotSource,
};
use energy_server::{EngineHandles, Service, ServiceOptions, app};

use energy_core::plan::PlanSummary;

/// Seconds per plan slot.
pub const SLOT: i64 = 900;
/// Tariff timezone used for savings days.
pub const TZ: Tz = chrono_tz::Europe::Amsterdam;

/// A seeded server and its engine handles.
pub struct Fixture {
    pub dir: TempDir,
    pub base: i64,
    pub ledger: Arc<Ledger>,
    pub service: Arc<Service>,
    pub handles: EngineHandles,
    pub plan_id: i64,
    pub pack_sn: String,
}

impl Fixture {
    pub fn new() -> Self {
        Self::with_heartbeat(Duration::from_secs(15))
    }

    pub fn with_heartbeat(heartbeat: Duration) -> Self {
        let dir = tempfile::tempdir().expect("a temp dir");
        let base = Utc::now().timestamp().div_euclid(SLOT) * SLOT;
        let path = dir.path().join("energy_rs.db");
        let ledger = Arc::new(Ledger::open(&LedgerConfig::at(path)).expect("the ledger opens"));
        let now = Utc::now().timestamp();
        let first = register_pack(&ledger, "AB2000-ONE", base);
        let second = register_pack(&ledger, "AB2000-TWO", base);
        seed_measurements(&ledger, now, first, second);
        let plan_id = seed_plans(&ledger, base);
        seed_dispatch(&ledger, now, first);
        seed_savings(&ledger);
        seed_settings(&ledger, base);

        let prices = MemoryPriceRepository::new();
        seed_prices(&prices, base);

        let (service, handles) = Service::new(
            Arc::clone(&ledger),
            Arc::new(prices),
            Settlement::new(energy_price::Contract::zonneplan_2026()),
            ServiceOptions::default().with_heartbeat(heartbeat),
        );
        let pack_sn = "AB2000-ONE".to_owned();
        let fixture = Self {
            dir,
            base,
            ledger,
            service,
            handles,
            plan_id,
            pack_sn,
        };
        fixture.publish_snapshot();
        fixture
    }

    /// Publish the standard snapshot (the engine's first tick).
    pub fn publish_snapshot(&self) {
        self.handles
            .publish(standard_snapshot(self.base, self.plan_id, &self.pack_sn));
    }

    /// The router with the shared service.
    pub fn router(&self) -> Router {
        app::router(Arc::clone(&self.service))
    }
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

fn register_pack(ledger: &Ledger, sn: &str, at: i64) -> i64 {
    let pack = Pack::new(sn, 1.92, at).with_limits(800.0, 800.0);
    ledger
        .packs()
        .expect("packs")
        .upsert(&pack)
        .expect("the pack is stored")
        .id
}

fn seed_measurements(ledger: &Ledger, now: i64, first: i64, second: i64) {
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&[
            Sample::new(first, now, 120.0, Some(60.0)),
            Sample::new(second, now, -80.0, Some(45.0)),
        ])
        .expect("samples are stored");
}

fn plan_summary() -> PlanSummary {
    PlanSummary {
        cost_estimate_eur: 1.234_567_8,
        baseline_cost_eur: 2.0,
        saved_eur: 0.765_432_2,
        charged_kwh: 2.5,
        discharged_kwh: 2.0,
        grid_charge_kwh: 1.5,
        pv_charge_kwh: 1.0,
        cycles: 0.5,
        terminal_target_kwh: None,
        infeasible_reason: None,
        export_floor_eur_per_kwh: None,
        zero_export: false,
        cost_basis_eur_per_kwh: None,
        peak_reserve_kwh: None,
        solar_reserve_kwh: 0.0,
        clipped_kwh: 0.0,
        gated_kwh: 0.0,
    }
}

fn plan_slots(base: i64) -> Vec<PlanSlot> {
    (0..4)
        .map(|idx| {
            let mut slot = PlanSlot::at(idx, base + idx * SLOT);
            slot.buy_all_in = 0.2;
            slot.feed_in = 0.08;
            slot.pv_kw = 1.0;
            slot.load_kw = 0.5;
            slot.battery_kw = if idx == 0 { 1.5 } else { -0.5 };
            slot.soc_kwh = 1.0 + 0.25 * idx as f64;
            slot.target_soc_kwh = 1.25 + 0.25 * idx as f64;
            if idx == 0 {
                "price_low".clone_into(&mut slot.reason);
            } else {
                "plan_soc_target".clone_into(&mut slot.reason);
            }
            slot
        })
        .collect()
}

fn seed_plans(ledger: &Ledger, base: i64) -> i64 {
    let plans = ledger.plans().expect("plans");
    let older = Plan::new(
        base - SLOT,
        "advisory",
        base - SLOT,
        base + 3 * SLOT,
        plan_summary().to_json(),
    );
    plans
        .insert(&older, &plan_slots(base - SLOT))
        .expect("older plan");
    let current = Plan::new(
        base,
        "dry_run",
        base,
        base + 4 * SLOT,
        plan_summary().to_json(),
    );
    plans
        .insert(&current, &plan_slots(base))
        .expect("current plan")
        .id
}

fn seed_dispatch(ledger: &Ledger, now: i64, pack: i64) {
    let log = ledger.dispatch().expect("dispatch");
    let requested = DispatchEntry::new(now - 60, Some(pack), 800.0, 0.0, "dry_run", "dry_run");
    log.insert(&requested).expect("requested row");
    let applied = DispatchEntry::new(now - 30, Some(pack), -400.0, -395.0, "dispatch", "auto")
        .verified(-395.0);
    log.insert(&applied).expect("applied row");
}

fn local_day(offset_days: i64) -> String {
    (Utc::now().with_timezone(&TZ).date_naive() + ChronoDuration::days(offset_days)).to_string()
}

fn seed_savings(ledger: &Ledger) {
    let savings = ledger.savings().expect("savings");
    let mut yesterday = SavingsDaily::new(local_day(-1), 1.5, 2.5);
    yesterday.import_kwh = 4.0;
    yesterday.export_kwh = 1.0;
    yesterday.pv_kwh = 6.0;
    yesterday.battery_charge_kwh = 2.0;
    yesterday.battery_discharge_kwh = 1.8;
    yesterday.coverage_hours = 24.0;
    yesterday.complete = true;
    yesterday.arbitrage_eur = Some(0.4);
    yesterday.self_consumption_eur = Some(0.5);
    yesterday.feed_in_eur = Some(0.2);
    yesterday.battery_balance_eur = Some(-0.1);
    savings.upsert(&yesterday).expect("yesterday");
    let mut last_week = SavingsDaily::incomplete(local_day(-7));
    last_week.import_kwh = 1.0;
    last_week.coverage_hours = 3.0;
    savings.upsert(&last_week).expect("incomplete day");
    let mut today = SavingsDaily::new(local_day(0), 0.5, 1.0);
    today.import_kwh = 2.0;
    today.pv_kwh = 3.0;
    today.coverage_hours = 6.0;
    savings.upsert(&today).expect("today");
}

fn seed_settings(ledger: &Ledger, at: i64) {
    let settings = ledger.settings().expect("settings");
    settings
        .set("control.mode", "dry_run", at)
        .expect("control mode");
    settings.set("reserve_soc_pct", "20", at).expect("reserve");
    // A private value that must never appear in an API response.
    settings
        .set("zonneplan_refresh_token", "SUPERSECRET", at)
        .expect("private setting");
}

fn seed_prices(prices: &MemoryPriceRepository, base: i64) {
    let slots = (0..192)
        .map(|idx| {
            let buy = 0.2 + idx as f64 * 0.000_1;
            Slot::new(
                base + idx * SLOT,
                buy,
                buy - 0.12,
                SlotSource::ZonneplanQuarterHourly,
            )
        })
        .collect::<Vec<_>>();
    let book = PriceBook::new(slots).expect("a contiguous book");
    prices.save_publication(&book).expect("the book is stored");
}

fn learning_profile(sn: &str) -> LearningProfile {
    serde_json::from_value(json!({
        "pack_id": sn,
        "capacity_kwh": 1.85,
        "capacity_confidence": 0.8,
        "capacity_applied": true,
        "capacity_decision": "applied",
        "band_low_kwh": 1.7,
        "band_high_kwh": 1.92,
        "round_trip": 0.9,
        "eta_charge": 0.948_683_3,
        "eta_discharge": 0.948_683_3,
        "efficiency_confidence": 0.7,
        "efficiency_applied": true,
        "sessions_used": 5,
        "updated_at": "2026-01-01T00:00:00Z",
    }))
    .expect("a learning profile")
}

fn cell_latch(sn: &str) -> CellLatch {
    serde_json::from_value(json!({
        "pack_id": sn,
        "blocked": true,
        "force_charge": false,
        "force_charge_w": 300,
        "since": "2026-01-01T00:00:00Z",
        "reason": "below_resume",
        "reason_at": "2026-01-01T00:05:00Z",
        "voltage_v": 3.01,
        "last_fresh_at": "2026-01-01T00:05:00Z",
    }))
    .expect("a cell latch")
}

fn site_snapshot() -> SiteSnapshot {
    SiteSnapshot {
        configured: true,
        sources: vec![
            SiteSource {
                kind: "grid".to_owned(),
                entity_id: "sensor.grid_energy".to_owned(),
                label: Some("Grid".to_owned()),
            },
            SiteSource {
                kind: "solar".to_owned(),
                entity_id: "sensor.pv_energy".to_owned(),
                label: None,
            },
        ],
        capabilities: vec!["battery_control".to_owned(), "savings".to_owned()],
    }
}

fn plan_snapshot(base: i64, plan_id: i64) -> PlanSnapshot {
    PlanSnapshot {
        plan_id,
        generated_at: base,
        horizon_start: base,
        horizon_end: base + 4 * SLOT,
        feasible: true,
        summary: plan_summary(),
        binding_reasons: vec!["price_low".to_owned()],
    }
}

fn pack_snapshot(sn: &str, base: i64) -> PackSnapshot {
    PackSnapshot {
        sn: sn.to_owned(),
        soc_pct: Some(60.0),
        power_w: Some(120.0),
        sampled_at: Some(base),
        stale: false,
        max_charge_w: Some(800.0),
        max_discharge_w: Some(800.0),
        profile: Some(learning_profile(sn)),
        latch: Some(cell_latch(sn)),
        override_state: None,
    }
}

fn standard_snapshot(base: i64, plan_id: i64, sn: &str) -> EngineSnapshot {
    EngineSnapshot {
        mode: Authority::DryRun,
        site: site_snapshot(),
        plan: Some(plan_snapshot(base, plan_id)),
        packs: vec![pack_snapshot(sn, base)],
        meter: Some(MeterSnapshot {
            at: base,
            grid_w: 250.0,
            pv_w: 1_800.0,
            load_w: 600.0,
            battery_w: Some(120.0),
        }),
        forecast: Some(ForecastSnapshot {
            at: base,
            horizon_start: base,
            pv_wh: 12_000.0,
            load_wh: 8_000.0,
            confidence: Some(0.75),
        }),
        last_tick: Some(base),
        faults: vec![FaultSnapshot {
            kind: "stale_forecast".to_owned(),
            message: "the PV forecast is older than the stale limit".to_owned(),
            since: base,
        }],
    }
}

/// A JSON request against the app router.
pub async fn send(
    router: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let (status, text) = send_text(router, method, uri, body).await;
    let value = serde_json::from_str(&text).unwrap_or(Value::Null);
    (status, value)
}

/// A request whose response is returned as text (for redaction checks).
pub async fn send_text(
    router: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, String) {
    let builder = Request::builder().method(method).uri(uri);
    let request = match body {
        None => builder.body(Body::empty()).expect("a request"),
        Some(value) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&value).expect("json")))
            .expect("a request"),
    };
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body is collected")
        .to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// Shorthand helpers.
pub async fn get(router: &Router, uri: &str) -> (StatusCode, Value) {
    send(router, Method::GET, uri, None).await
}

pub async fn post(router: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    send(router, Method::POST, uri, Some(body)).await
}

pub async fn patch(router: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    send(router, Method::PATCH, uri, Some(body)).await
}

/// A service over an empty in-memory ledger, for the 404 paths.
pub fn empty_service() -> Arc<Service> {
    let (service, handles) = Service::new(
        Arc::new(Ledger::open_in_memory().expect("in-memory ledger")),
        Arc::new(MemoryPriceRepository::new()),
        Settlement::new(energy_price::Contract::zonneplan_2026()),
        ServiceOptions::default(),
    );
    drop(handles);
    service
}

/// A router over [`empty_service`].
pub fn empty_router() -> Router {
    app::router(empty_service())
}

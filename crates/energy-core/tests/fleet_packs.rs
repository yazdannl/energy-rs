//! Four zenSDK devices are four ledger packs: own SoC, own learning, own command.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_possible_wrap
)]

mod support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use energy_battery::learning::{LearningDecision, LearningProfile, LearningStore};
use energy_battery::report::parse_report;
use energy_battery::transport::{WriteAck, WriteRequest};
use energy_battery::{
    BatteryError, BoxFuture, ControlLimits, DeviceController, DeviceIo, FleetClient, Report,
};
use energy_core::dispatch::{
    BatteryFleet, DeviceFleet, Dispatcher, FleetView, MeterReading, Overrides, PackCatalog,
    TickInput,
};
use energy_core::safety::{Authority, SafetyLimits};
use energy_ledger::Ledger;
use support::fakes::{FakeSink, NOW, plan};

const SERIALS: [&str; 4] = ["DEV-1", "DEV-2", "DEV-3", "DEV-4"];

/// A transport that always answers the same report and refuses writes.
struct FakeIo {
    report: Report,
}

impl FakeIo {
    fn new(report: Report) -> Self {
        Self { report }
    }
}

impl DeviceIo for FakeIo {
    fn sn(&self) -> &str {
        &self.report.sn
    }

    fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>> {
        Box::pin(async move { Ok(self.report.clone()) })
    }

    fn write(&self, _request: WriteRequest) -> BoxFuture<'_, Result<WriteAck, BatteryError>> {
        let sn = self.report.sn.clone();
        Box::pin(async move { Err(BatteryError::NotRead { sn }) })
    }
}

/// A learning store with scripted profiles that records every lookup.
struct FakeLearning {
    profiles: BTreeMap<String, LearningProfile>,
    seen: Mutex<Vec<String>>,
}

impl FakeLearning {
    fn new(profiles: BTreeMap<String, LearningProfile>) -> Self {
        Self {
            profiles,
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl LearningStore for FakeLearning {
    fn load(&self, pack_id: &str) -> Result<Option<LearningProfile>, BatteryError> {
        self.seen
            .lock()
            .expect("learning lookups")
            .push(pack_id.to_owned());
        Ok(self.profiles.get(pack_id).cloned())
    }

    fn save(&self, _profile: &LearningProfile) -> Result<(), BatteryError> {
        Ok(())
    }
}

fn report(sn: &str, soc_pct: i64, pack_sn: &str) -> Report {
    let document = serde_json::json!({
        "sn": sn,
        "timestamp": NOW,
        "messageId": 1,
        "properties": {
            "electricLevel": soc_pct,
            "chargeMaxLimit": 800,
            "inverseMaxPower": 800,
            "minSoc": 50
        },
        "packData": [{
            "sn": pack_sn,
            "socLevel": soc_pct,
            "minVol": 330,
            "maxVol": 340,
            "totalVol": 3350,
            "batcur": 10
        }]
    });
    parse_report(document.to_string().as_bytes()).expect("report parses")
}

fn profile(id: &str, capacity_kwh: f64, round_trip: f64) -> LearningProfile {
    let eta = round_trip.sqrt();
    LearningProfile {
        pack_id: id.to_owned(),
        capacity_kwh: Some(capacity_kwh),
        capacity_confidence: 0.9,
        capacity_applied: true,
        capacity_decision: LearningDecision::Applied,
        band_low_kwh: 1.44,
        band_high_kwh: 2.40,
        round_trip,
        eta_charge: eta,
        eta_discharge: eta,
        efficiency_confidence: 0.8,
        efficiency_applied: true,
        sessions_used: 4,
        updated_at: DateTime::<Utc>::UNIX_EPOCH,
    }
}

/// One fleet of four devices, each with its own pack label and learning profile for DEV-1 only.
struct FourPacks {
    fleet: BatteryFleet<FakeIo>,
    learning: Arc<FakeLearning>,
    ledger: Arc<Ledger>,
}

impl FourPacks {
    fn new() -> Self {
        let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
        let mut profiles = BTreeMap::new();
        profiles.insert("DEV-1".to_owned(), profile("DEV-1", 1.70, 0.90));
        let learning = Arc::new(FakeLearning::new(profiles));
        let catalog = Arc::new(PackCatalog::new(
            Arc::clone(&ledger),
            Some(learning.clone()),
        ));
        let devices: Vec<Arc<DeviceController<FakeIo>>> = SERIALS
            .iter()
            .enumerate()
            .map(|(index, sn)| {
                let pack_sn = format!("C0AB200000{index}");
                let io = FakeIo::new(report(sn, 40 + 10 * (index as i64), &pack_sn));
                Arc::new(DeviceController::new(io, ControlLimits::unlimited()))
            })
            .collect();
        let client = Arc::new(FleetClient::with_interval(devices, Duration::from_secs(1)));
        Self {
            fleet: BatteryFleet::new(client, catalog, 180.0),
            learning,
            ledger,
        }
    }

    async fn view(&self) -> FleetView {
        self.fleet.view(NOW).await.expect("fleet view")
    }
}

#[tokio::test]
async fn four_devices_are_four_packs_with_independent_learning() {
    let four = FourPacks::new();
    let view = four.view().await;
    assert_eq!(view.packs.len(), 4);
    let ids: Vec<i64> = view
        .packs
        .iter()
        .map(|pack| pack.ledger_id.expect("id"))
        .collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 4, "four ledger packs, not one hub row");
    assert_eq!(
        view.packs
            .iter()
            .map(|pack| pack.soc_pct.expect("soc"))
            .collect::<Vec<f64>>(),
        vec![40.0, 50.0, 60.0, 70.0]
    );
    assert_eq!(
        view.packs[0].spec.capacity_kwh, 1.70,
        "DEV-1 learned capacity"
    );
    assert!((view.packs[0].spec.eta_charge - 0.90_f64.sqrt()).abs() < 1e-12);
    for pack in &view.packs[1..] {
        assert_eq!(pack.spec.capacity_kwh, 1.92, "its own packData label");
        assert!((pack.spec.eta_charge - 0.81_f64.sqrt()).abs() < 1e-12);
    }
    let rows = four.ledger.packs().expect("packs").all().expect("rows");
    let mut serials: Vec<String> = rows.into_iter().map(|row| row.sn).collect();
    serials.sort();
    assert_eq!(serials, SERIALS.to_vec());
    let seen = four.learning.seen.lock().expect("learning lookups").clone();
    let mut seen_unique = seen.clone();
    seen_unique.sort();
    seen_unique.dedup();
    assert_eq!(
        seen_unique,
        SERIALS.to_vec(),
        "one lookup per device serial"
    );
}

#[tokio::test]
async fn four_packs_get_four_independent_commands() {
    let four = FourPacks::new();
    let view = four.view().await;
    // One pack cannot take this: the request needs all four, so each gets its own command no
    // matter how the fleet is staggered.
    let plan = plan(NOW, 3.2, 2.0);
    let sink = FakeSink::new();
    let dispatcher = Dispatcher::new(SafetyLimits::default(), true, 10);
    let overrides = Overrides::none();
    let meter = MeterReading::default();
    let input = TickInput {
        now: NOW,
        mode: Authority::Auto,
        plan: Some(&plan),
        fleet: &view,
        meter: &meter,
        latches: &[],
        overrides: &overrides,
        stale: false,
        progress: None,
    };
    let outcome = dispatcher.tick(&input, &sink).await;
    assert_eq!(outcome.packs.len(), 4);
    let commands = sink.commands();
    assert_eq!(commands.len(), 4, "one command per device");
    let mut addressed: Vec<String> = commands.iter().map(|(sn, _)| sn.clone()).collect();
    addressed.sort();
    assert_eq!(addressed, SERIALS.to_vec());
    for pack in &outcome.packs {
        assert!(
            pack.requested_w > 0.0,
            "{} got {} W",
            pack.pack_id,
            pack.requested_w
        );
    }
}

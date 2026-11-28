//! The dispatch catalog's efficiency is the shared learned > manual > default precedence.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use std::sync::{Arc, Mutex};

use energy_battery::BatteryError;
use energy_battery::learning::{LearningProfile, LearningStore};
use energy_core::dispatch::PackCatalog;
use energy_ledger::{Ledger, Pack};

const SN: &str = "DEV-ONE";
const LABEL_KWH: f64 = 1.92;

/// A learning store with one scripted profile.
struct MemoryLearning {
    profile: Mutex<Option<LearningProfile>>,
}

impl MemoryLearning {
    fn new(profile: LearningProfile) -> Self {
        Self {
            profile: Mutex::new(Some(profile)),
        }
    }
}

impl LearningStore for MemoryLearning {
    fn load(&self, _pack_id: &str) -> Result<Option<LearningProfile>, BatteryError> {
        Ok(self.profile.lock().expect("learning profile").clone())
    }

    fn save(&self, profile: &LearningProfile) -> Result<(), BatteryError> {
        *self.profile.lock().expect("learning profile") = Some(profile.clone());
        Ok(())
    }
}

/// A profile with a fresh measurement, or with only the typed seed when `applied` is false.
fn profile(round_trip: f64, applied: bool) -> LearningProfile {
    serde_json::from_value(serde_json::json!({
        "pack_id": SN,
        "capacity_kwh": 1.8,
        "capacity_confidence": 0.8,
        "capacity_applied": true,
        "capacity_decision": "applied",
        "band_low_kwh": 1.7,
        "band_high_kwh": 1.92,
        "round_trip": round_trip,
        "eta_charge": round_trip.sqrt(),
        "eta_discharge": round_trip.sqrt(),
        "efficiency_confidence": if applied { 0.8 } else { 0.0 },
        "efficiency_applied": applied,
        "efficiency_diagnostics": {},
        "sessions_used": 3,
        "soc_points_covered": 100.0,
        "cycles_used": 3,
        "capacity_source": "own",
        "updated_at": "2026-01-01T00:00:00Z"
    }))
    .expect("a learning profile")
}

/// One pack row and an optional learning profile, as the engine sees them.
fn catalog(manual: Option<f64>, profile: Option<LearningProfile>) -> PackCatalog {
    catalog_with(manual, profile, None)
}

/// The third input is a published ledger measurement `(round trip, stamp)`.
fn catalog_with(
    manual: Option<f64>,
    profile: Option<LearningProfile>,
    published: Option<(f64, i64)>,
) -> PackCatalog {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let packs = ledger.packs().expect("packs");
    let id = packs
        .upsert(&Pack::new(SN, LABEL_KWH, 100))
        .expect("the pack is registered")
        .id;
    if let Some(value) = manual {
        packs
            .set_manual_round_trip(SN, Some(value))
            .expect("the manual value is stored");
    }
    if let Some((value, at)) = published {
        packs
            .set_learning(id, None, value, 0.8, at)
            .expect("the measurement is published");
    }
    drop(packs);
    let learning =
        profile.map(|value| Arc::new(MemoryLearning::new(value)) as Arc<dyn LearningStore>);
    PackCatalog::new(ledger, learning)
}

fn spec(catalog: &PackCatalog) -> energy_core::plan::PackSpec {
    catalog.spec(SN, LABEL_KWH, 800.0, 800.0)
}

#[test]
fn dispatch_catalog_uses_manual_round_trip_without_learning() {
    let catalog = catalog(Some(0.83), None);
    let pack = spec(&catalog);
    assert!((pack.eta_charge - 0.83_f64.sqrt()).abs() < 1e-12);
    assert!((pack.eta_discharge - 0.83_f64.sqrt()).abs() < 1e-12);
}

#[test]
fn dispatch_catalog_ignores_an_unmeasured_learning_profile() {
    let catalog = catalog(Some(0.83), Some(profile(0.95, false)));
    let pack = spec(&catalog);
    assert!(
        (pack.eta_charge - 0.83_f64.sqrt()).abs() < 1e-12,
        "a typed seed in the profile is not a measurement"
    );
}

#[test]
fn dispatch_catalog_prefers_a_confident_learned_round_trip() {
    let catalog = catalog(Some(0.83), Some(profile(0.95, true)));
    let pack = spec(&catalog);
    assert!((pack.eta_charge - 0.95_f64.sqrt()).abs() < 1e-12);
}

#[test]
fn dispatch_catalog_prefers_a_published_ledger_round_trip() {
    let catalog = catalog_with(Some(0.83), None, Some((0.94, 1)));
    let pack = spec(&catalog);
    assert!((pack.eta_charge - 0.94_f64.sqrt()).abs() < 1e-12);
}

#[test]
fn dispatch_catalog_ignores_out_of_range_manual_round_trips() {
    for bad in [0.4, 1.2, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let catalog = catalog(Some(bad), None);
        let pack = spec(&catalog);
        assert!(
            (pack.eta_charge - 0.81_f64.sqrt()).abs() < 1e-12,
            "bad value {bad} must fall back to the typed default"
        );
    }
}

//! Benchmarks: artifact load, the 800-vector golden walk and one served slot.

// Test code: a failed expectation is the report, so `expect` is the intended API here.
#![allow(clippy::expect_used)]

use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use energy_forecast::features::Features;
use energy_forecast::model::PvModel;
use energy_forecast::physics::Site;
use energy_forecast::pv::{SlotInputs, estimate_slot};

fn artifact_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/pv_base.json")
}

fn golden_features() -> Vec<Features> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pv_golden.jsonl");
    let text = std::fs::read_to_string(path).expect("golden fixture is committed");
    text.lines()
        .filter_map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).ok()?;
            let items = value.get("features")?.as_array()?;
            let values: Vec<f64> = items.iter().filter_map(serde_json::Value::as_f64).collect();
            Features::from_slice(&values).ok()
        })
        .collect()
}

fn bench_walk(criterion: &mut Criterion) {
    let model = PvModel::load_path(&artifact_path()).expect("artifact is committed");
    let rows = golden_features();
    criterion.bench_function("walk/800_golden_vectors", |bencher| {
        bencher.iter(|| {
            let mut total = 0.0;
            for row in &rows {
                if let Ok(ratio) = model.ratio(row) {
                    total += ratio;
                }
            }
            total
        });
    });
}

fn bench_load(criterion: &mut Criterion) {
    criterion.bench_function("artifact/load", |bencher| {
        bencher.iter(|| PvModel::load_path(&artifact_path()).expect("artifact is committed"));
    });
}

fn bench_slot(criterion: &mut Criterion) {
    let model = PvModel::load_path(&artifact_path()).expect("artifact is committed");
    let moment = chrono::DateTime::parse_from_rfc3339("2026-06-21T12:00:00+00:00")
        .expect("timestamp is valid")
        .to_utc();
    let site = Site::new(52.0, 5.0, 35.0, 180.0);
    let mut inputs = SlotInputs::new(moment, site, 4000.0, 4000.0, 640.0, 18.0);
    inputs.cloud_pct = 40.0;
    inputs.attenuation = 0.9;
    criterion.bench_function("slot/estimate", |bencher| {
        bencher.iter(|| estimate_slot(&model, &inputs).expect("model walks"));
    });
}

criterion_group!(benches, bench_load, bench_walk, bench_slot);
criterion_main!(benches);

//! Tests for the one-time empty-source notice: log volume, names and unchanged arithmetic.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fmt::{self, Write};
use std::sync::{Arc, Mutex};

use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

use super::{clear, report, skipped_sources};
use crate::plan_source::ha_series::sum_sources;

/// The registry is process-wide; keep these tests from clearing each other's state.
static TEST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<String>>>);

struct Fields(String);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let _ = write!(self.0, " {}={value:?}", field.name());
    }
}

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields(String::new());
        event.record(&mut fields);
        let line = format!("{} {}", event.metadata().level(), fields.0);
        self.0.lock().expect("capture").push(line);
    }
}

fn captured<T>(action: impl FnOnce() -> T) -> (Vec<String>, T) {
    clear();
    let capture = Capture::default();
    let registry = tracing_subscriber::registry().with(capture.clone());
    let result = tracing::subscriber::with_default(registry, action);
    let lines = capture.0.lock().expect("capture").clone();
    (lines, result)
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn one_empty_source_is_announced_once_until_it_recovers() {
    let _guard = TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let configured = ids(&["sensor.p1_tariff_1", "p1_history:grid_import"]);
    let empty = ids(&["p1_history:grid_import"]);

    let (lines, ()) = captured(|| {
        report("grid_import", &configured, &empty);
        report("grid_import", &configured, &empty);
        report("grid_import", &configured, &[]); // the source published again
        report("grid_import", &configured, &empty); // then went quiet again
    });

    let info: Vec<&String> = lines
        .iter()
        .filter(|line| line.starts_with("INFO"))
        .collect();
    assert_eq!(info.len(), 2, "one notice per silent spell: {lines:?}");
    assert!(
        info[0].contains("grid_import") && info[0].contains("p1_history:grid_import"),
        "the notice must name the series and the source: {}",
        info[0]
    );
    assert!(!lines.iter().any(|line| line.starts_with("WARN")));
    assert_eq!(skipped_sources().len(), 1);
    assert_eq!(skipped_sources()[0].entity_id, "p1_history:grid_import");
}

#[test]
fn an_all_empty_group_still_surfaces_a_warning() {
    let _guard = TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let empty = ids(&["p1_history:grid_import", "p1_history:grid_export"]);
    let (lines, ()) = captured(|| {
        report("grid_import", &empty, &empty);
        report("grid_import", &empty, &empty);
    });

    let warnings: Vec<&String> = lines
        .iter()
        .filter(|line| line.starts_with("WARN"))
        .collect();
    assert_eq!(
        warnings.len(),
        2,
        "a missing series stays visible: {lines:?}"
    );
    assert!(warnings[0].contains("p1_history:grid_import"));
    assert!(!lines.iter().any(|line| line.starts_with("INFO")));
    assert_eq!(skipped_sources().len(), 2);
}

#[test]
fn the_sum_ignores_an_empty_source_and_names_it_once() {
    let _guard = TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let healthy = BTreeMap::from([(1_800_000_000, 0.10)]);
    let legacy = BTreeMap::new();
    let configured = ids(&["sensor.p1_tariff_1", "p1_history:grid_import"]);

    let (lines, sum) = captured(|| {
        let first = sum_sources(
            &[healthy.clone(), legacy.clone()],
            &configured,
            "grid_import",
        );
        let second = sum_sources(&[healthy.clone(), legacy], &configured, "grid_import");
        (first, second)
    });

    assert_eq!(sum.0.get(&1_800_000_000), Some(&0.10));
    assert_eq!(sum.1, sum.0, "the empty source changes neither sum");
    let info: Vec<&String> = lines
        .iter()
        .filter(|line| line.starts_with("INFO"))
        .collect();
    assert_eq!(info.len(), 1, "not a WARN per solve: {lines:?}");
    assert!(info[0].contains("p1_history:grid_import"));
}

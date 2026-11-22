//! One-time notices for configured grid statistics sources that publish no complete quarter.
//!
//! A legacy hourly-only counter left in the HA Energy dashboard is empty in every 48 h window, so
//! a per-solve WARN would flood the log. The arithmetic still skips it (see `ha_series::sum_series`);
//! only the reporting changes: one INFO notice per newly empty source set, naming the IDs and
//! repeating when a source recovers and then goes empty again. An all-empty group is different --
//! the series really is missing and the plan will be refused -- so it keeps a visible WARN on every
//! solve. The same registry backs
//! `GET /api/v1/site`, where an operator can see which sources are currently being ignored.

use std::collections::BTreeSet;
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde::Serialize;

/// A configured grid source that published no complete quarter in the latest window.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedSource {
    /// The series the source was configured for (`grid_import` or `grid_export`).
    pub series: &'static str,
    /// The Home Assistant statistics id.
    pub entity_id: String,
}

#[derive(Debug, Default)]
struct Notices {
    /// Currently empty sources, for the status endpoint.
    empty: BTreeSet<SkippedSource>,
    /// Sources already announced at INFO; removed when the source publishes again.
    announced: BTreeSet<SkippedSource>,
}

static NOTICES: OnceLock<Mutex<Notices>> = OnceLock::new();

fn registry() -> &'static Mutex<Notices> {
    NOTICES.get_or_init(|| Mutex::new(Notices::default()))
}

fn lock() -> MutexGuard<'static, Notices> {
    match registry().lock() {
        Ok(guard) => guard,
        // A panic while holding the lock must not take down planning over a log message.
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Update the registry and return the sources that have not been announced yet.
pub(crate) fn announce(series: &'static str, empty: &[String]) -> Vec<SkippedSource> {
    let mut notices = lock();
    // Keep entries for sources that are still empty: re-announcing them on every solve would be
    // the same log spam this replaces. Drop only this series' entries for sources that recovered
    // or left the dashboard, so a later silent spell gets its own notice.
    let still_empty =
        |item: &SkippedSource| item.series != series || empty.contains(&item.entity_id);
    notices.empty.retain(still_empty);
    notices.announced.retain(still_empty);
    let mut fresh = Vec::new();
    for entity in empty {
        let item = SkippedSource {
            series,
            entity_id: entity.clone(),
        };
        notices.empty.insert(item.clone());
        if notices.announced.insert(item.clone()) {
            fresh.push(item);
        }
    }
    fresh
}

/// The sources currently empty, for the `/api/v1/site` status field.
pub fn skipped_sources() -> Vec<SkippedSource> {
    lock().empty.iter().cloned().collect()
}

/// Report this group's empty sources: INFO once per source, WARN while the whole group is empty.
pub(crate) fn report(series: &'static str, entities: &[String], empty: &[String]) {
    let fresh = announce(series, empty);
    if !entities.is_empty() && empty.len() == entities.len() {
        warn_all_empty(series, entities);
    } else {
        info_once(series, &fresh);
    }
}

fn warn_all_empty(series: &'static str, entities: &[String]) {
    let ids = entities.join(", ");
    tracing::warn!(
        series,
        source_ids = %ids,
        "all configured grid sources are empty; the series is missing"
    );
}

fn info_once(series: &'static str, fresh: &[SkippedSource]) {
    if fresh.is_empty() {
        return;
    }
    let ids = fresh
        .iter()
        .map(|source| source.entity_id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    tracing::info!(
        series,
        source_ids = %ids,
        "empty grid sources are being ignored; no slots were invented"
    );
}

#[cfg(test)]
pub(crate) fn clear() {
    let mut notices = lock();
    notices.empty.clear();
    notices.announced.clear();
}

#[cfg(test)]
#[path = "source_notice_tests.rs"]
mod tests;

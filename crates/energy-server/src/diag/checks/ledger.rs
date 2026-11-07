//! The ledger file: schema version, row counts, freshness of the last tick and retention headroom.
//!
//! Read through a `query_only` connection: the doctor never migrates, trims or registers a pack.

use energy_ledger::LedgerConfig;
use energy_ledger::stats::SUPPORTED_SCHEMA_VERSION;
use energy_ledger::{LedgerStats, RetentionPolicy};

use crate::bootstrap::options::Options;
use crate::diag::check::Check;
use crate::diag::status::Status;

/// `now` is the reference for the ages; the ledger rows carry UTC epoch seconds.
pub fn checks(options: &Options, now: i64) -> Vec<Check> {
    let path = options.database();
    if !path.exists() {
        return vec![Check::warn(
            "ledger",
            format!("{} does not exist; start the add-on once", path.display()),
        )];
    }
    let stats = match LedgerStats::read(&path) {
        Ok(stats) => stats,
        Err(error) => return vec![Check::fail("ledger", error.to_string())],
    };
    let policy = LedgerConfig::at(&path).retention;
    vec![
        file(&stats, &path.display().to_string()),
        rows(&stats),
        tick(&stats, now),
        retention(&stats, policy),
    ]
}

fn file(stats: &LedgerStats, options_path: &str) -> Check {
    let version = stats.schema_version;
    let detail = format!(
        "schema {version}/{SUPPORTED_SCHEMA_VERSION}, {:.1} MiB, {}",
        stats.size_bytes as f64 / (1024.0 * 1024.0),
        options_path,
    );
    if version == SUPPORTED_SCHEMA_VERSION {
        Check::pass("ledger", detail)
    } else if version == 0 {
        Check::fail(
            "ledger",
            format!("{detail}; the file was never migrated, start the add-on once"),
        )
    } else {
        Check::fail(
            "ledger",
            format!("{detail}; schema mismatch, start the matching add-on version"),
        )
    }
}

fn rows(stats: &LedgerStats) -> Check {
    let detail = format!(
        "packs {}, 1m {}, 5m {}, sessions {}, plans {}, dispatch {}, events {}, savings {}",
        stats.packs,
        stats.measurements_1m,
        stats.measurements_5m,
        stats.sessions,
        stats.plans,
        stats.dispatch_log,
        stats.events,
        stats.savings_daily,
    );
    let status = if stats.is_empty() {
        Status::Warn
    } else {
        Status::Pass
    };
    match status {
        Status::Warn => Check::warn("ledger rows", format!("{detail}; no data yet")),
        _ => Check::pass("ledger rows", detail),
    }
}

fn tick(stats: &LedgerStats, now: i64) -> Check {
    if stats.last_measurement_at.is_none() && stats.last_plan_at.is_none() {
        return Check::warn(
            "ledger last tick",
            "no measurement and no plan yet; the engine has not completed a tick",
        );
    }
    let detail = format!(
        "last sample {}, last plan {}, last dispatch {}, last event {}",
        age(stats.last_measurement_at, now),
        age(stats.last_plan_at, now),
        age(stats.last_dispatch_at, now),
        age(stats.last_event_at, now),
    );
    let plan_age = stats.last_plan_at.map(|at| now.saturating_sub(at));
    let sample_age = stats.last_measurement_at.map(|at| now.saturating_sub(at));
    let status = if plan_age.is_none_or(|age| age > 1_800) || sample_age.is_none_or(|age| age > 300)
    {
        Status::Warn
    } else {
        Status::Pass
    };
    match status {
        Status::Warn => Check::warn("ledger last tick", detail),
        _ => Check::pass("ledger last tick", detail),
    }
}

fn retention(stats: &LedgerStats, policy: RetentionPolicy) -> Check {
    let detail = format!(
        "plans {}/{}, dispatch {}/{}, events {}/{}; 1m {} d, 5m {} d",
        stats.plans,
        policy.plans_kept,
        stats.dispatch_log,
        policy.dispatch_kept,
        stats.events,
        policy.events_kept,
        policy.one_minute_days,
        policy.five_minute_days,
    );
    let over = stats.plans > policy.plans_kept
        || stats.dispatch_log > policy.dispatch_kept
        || stats.events > policy.events_kept;
    if over {
        Check::warn("ledger retention", format!("{detail}; a trim is due"))
    } else {
        Check::pass("ledger retention", detail)
    }
}

fn age(at: Option<i64>, now: i64) -> String {
    at.map_or_else(
        || "-".to_owned(),
        |at| {
            let seconds = now.saturating_sub(at).max(0);
            if seconds < 3_600 {
                format!("{seconds}s ago")
            } else {
                format!("{:.1} h ago", seconds as f64 / 3600.0)
            }
        },
    )
}

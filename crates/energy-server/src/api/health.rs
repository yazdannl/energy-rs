//! `GET /api/v1/health` — the Supervisor watchdog's liveness signal.
//!
//! The endpoint reads only the last published in-memory snapshot and two `Copy` boot stamps. It
//! never calls Home Assistant, a pack or the ledger, and never takes an engine lock, so a stalled
//! tick loop cannot prevent this answer. A stale tick or a stale pack sample answers HTTP 503 so the
//! Supervisor restarts the add-on instead of leaving a stale charge standing.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::Utc;
use serde::Serialize;

use crate::api::dto::Timestamp;
use crate::service::Service;
use crate::service_options::HEALTH_STARTUP_GRACE;

use energy_core::safety::Authority;
use energy_core::stream::EngineSnapshot;

/// Health payload. `status` and `version` are the Supervisor contract; the rest are additive.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Health {
    status: &'static str,
    version: &'static str,
    mode: Authority,
    last_tick_at: Option<Timestamp>,
    reason: Option<&'static str>,
    tick_stalled_since: Option<Timestamp>,
    sampler_stalled_since: Option<Timestamp>,
    age_seconds: Option<i64>,
}

impl Health {
    fn ok(mode: Authority, last_tick: Option<i64>) -> Self {
        Self {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
            mode,
            last_tick_at: Timestamp::lenient(last_tick),
            reason: None,
            tick_stalled_since: None,
            sampler_stalled_since: None,
            age_seconds: None,
        }
    }

    fn degraded(self, reason: &'static str, since: i64, age_seconds: i64) -> Self {
        let mut result = self;
        result.status = "degraded";
        result.reason = Some(reason);
        result.age_seconds = Some(age_seconds.max(0));
        if reason == "tick_stalled" {
            result.tick_stalled_since = Timestamp::lenient(Some(since));
        } else {
            result.sampler_stalled_since = Timestamp::lenient(Some(since));
        }
        result
    }
}

/// The verdict, independent of axum so it can be tested without a server.
fn evaluate(
    snapshot: &EngineSnapshot,
    now: i64,
    stale_after: Duration,
    booted_at: i64,
    boot_instant: Instant,
) -> (StatusCode, Health) {
    let health = Health::ok(snapshot.mode, snapshot.last_tick);
    // Startup is not a liveness failure: discovery, the first fleet read and the first solve may
    // legitimately take longer than one tick, and there is no other long operation in this engine.
    if boot_instant.elapsed() < HEALTH_STARTUP_GRACE
        || now.saturating_sub(booted_at)
            < i64::try_from(HEALTH_STARTUP_GRACE.as_secs()).unwrap_or(i64::MAX)
    {
        return (StatusCode::OK, health);
    }
    let bound = i64::try_from(stale_after.as_secs().max(1)).unwrap_or(i64::MAX);
    if let Some(last_tick) = snapshot.last_tick {
        let age = now.saturating_sub(last_tick);
        if age > bound {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                health.degraded("tick_stalled", last_tick, age),
            );
        }
    }
    let newest_sample = snapshot
        .packs
        .iter()
        .filter_map(|pack| pack.sampled_at)
        .max();
    if let Some(sampled_at) = newest_sample {
        let age = now.saturating_sub(sampled_at);
        if age > bound {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                health.degraded("sampler_stalled", sampled_at, age),
            );
        }
    }
    (StatusCode::OK, health)
}

/// `GET /api/v1/health`
pub(crate) async fn health(State(service): State<Arc<Service>>) -> (StatusCode, Json<Health>) {
    let snapshot = service.snapshot();
    let (booted_at, boot_instant) = service.boot();
    let (status, body) = evaluate(
        &snapshot,
        Utc::now().timestamp(),
        service.health_stale_after(),
        booted_at,
        boot_instant,
    );
    (status, Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use energy_core::stream::{EngineSnapshot, PackSnapshot};

    fn snapshot(tick: Option<i64>, sample: Option<i64>) -> EngineSnapshot {
        let mut value = EngineSnapshot {
            last_tick: tick,
            ..EngineSnapshot::default()
        };
        if let Some(sample) = sample {
            value.packs.push(PackSnapshot {
                sampled_at: Some(sample),
                ..PackSnapshot::default()
            });
        }
        value
    }

    #[allow(clippy::unchecked_time_subtraction)]
    fn verdict(now: i64, tick: Option<i64>, sample: Option<i64>) -> (StatusCode, Health) {
        evaluate(
            &snapshot(tick, sample),
            now,
            Duration::from_secs(120),
            now - 200,
            Instant::now() - Duration::from_secs(200),
        )
    }

    #[test]
    fn fresh_tick_and_sample_are_ok() {
        let (status, body) = verdict(1_000, Some(990), Some(995));
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.status, "ok");
        assert_eq!(body.reason, None);
    }

    #[test]
    fn a_stale_tick_degrades_with_age_and_since() {
        let (status, body) = verdict(1_000, Some(879), Some(995));
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body.reason, Some("tick_stalled"));
        assert_eq!(body.age_seconds, Some(121));
        assert!(body.tick_stalled_since.is_some());
    }

    #[test]
    fn startup_grace_suppresses_the_check() {
        let (status, body) = evaluate(
            &snapshot(Some(0), Some(0)),
            10,
            Duration::from_secs(120),
            0,
            Instant::now(),
        );
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.status, "ok");
    }

    #[test]
    fn a_stale_sample_degrades_even_with_a_fresh_tick() {
        let (status, body) = verdict(1_000, Some(999), Some(879));
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body.reason, Some("sampler_stalled"));
        assert_eq!(body.age_seconds, Some(121));
        assert!(body.sampler_stalled_since.is_some());
    }
}

//! `GET /api/v1/health` — the Supervisor watchdog's liveness signal.
//!
//! The endpoint reads only the last published in-memory snapshot and two `Copy` boot stamps. It
//! never calls Home Assistant, a pack or the ledger, and never takes an engine lock, so a stalled
//! tick loop cannot prevent this answer. A stalled tick or a blind fleet answers HTTP 503. A single
//! offline pack is diagnostics only: a restart cannot repair flaky WiFi, but it can interrupt a
//! live charge or discharge.

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
use energy_core::stream::{EngineSnapshot, PackSnapshot};

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
    /// Only the serials with an active, unresolved uncertain write.
    uncertain_packs: Vec<String>,
    /// Required packs whose own sample is stale or missing. Never watchdog-triggering alone.
    stalled_packs: Vec<StalledPack>,
    /// True when diagnostics degraded the payload while the watchdog verdict stayed OK.
    degraded: bool,
    age_seconds: Option<i64>,
}

/// One required pack that the engine cannot currently observe; last seen is operator evidence.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StalledPack {
    sn: String,
    sample_age_seconds: i64,
    last_seen_at: Option<Timestamp>,
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
            uncertain_packs: Vec::new(),
            stalled_packs: Vec::new(),
            degraded: false,
            age_seconds: None,
        }
    }

    fn with_reason(mut self, reason: &'static str) -> Self {
        self.reason = Some(reason);
        self
    }

    fn degraded(self, reason: &'static str, since: i64, age_seconds: i64) -> Self {
        let mut result = self;
        result.status = "degraded";
        result.reason = Some(reason);
        result.age_seconds = Some(age_seconds.max(0));
        result.degraded = true;
        if reason == "tick_stalled" {
            result.tick_stalled_since = Timestamp::lenient(Some(since));
        } else {
            result.sampler_stalled_since = Timestamp::lenient(Some(since));
        }
        result
    }
}

/// Only an active episode counts. `last_control_fault` is durable history: treating it as current
/// made one recovered pack label the whole fleet indefinitely.
fn uncertain_packs(snapshot: &EngineSnapshot) -> Vec<String> {
    snapshot
        .packs
        .iter()
        .filter(|pack| {
            pack.active_control_fault
                .as_deref()
                .is_some_and(|fault| fault.starts_with("uncertain_write:"))
        })
        .map(|pack| pack.sn.clone())
        .collect()
}

/// Packs the operator requires: the ledger configuration, not merely whatever the sampler saw.
/// A disabled or absent pack cannot degrade health. Older snapshots without the list fall back to
/// the reported packs so the diagnostic never silently disappears.
fn required(snapshot: &EngineSnapshot) -> Vec<&PackSnapshot> {
    if snapshot.required_packs.is_empty() {
        return snapshot.packs.iter().collect();
    }
    snapshot
        .packs
        .iter()
        .filter(|pack| snapshot.required_packs.contains(&pack.sn))
        .collect()
}

/// Per-pack freshness. A missing sample is stale since boot, never since the newest neighbour.
fn stalled(packs: &[&PackSnapshot], now: i64, bound: i64, booted_at: i64) -> Vec<StalledPack> {
    packs
        .iter()
        .filter_map(|pack| {
            let (age, at) = match pack.sampled_at {
                Some(at) => (now.saturating_sub(at), Some(at)),
                None => (now.saturating_sub(booted_at), None),
            };
            (age > bound).then_some(StalledPack {
                sn: pack.sn.clone(),
                sample_age_seconds: age,
                last_seen_at: Timestamp::lenient(at),
            })
        })
        .collect()
}

/// The verdict, independent of axum so it can be tested without a server.
fn evaluate(
    snapshot: &EngineSnapshot,
    now: i64,
    stale_after: Duration,
    booted_at: i64,
    boot_instant: Instant,
) -> (StatusCode, Health) {
    let mut health = Health::ok(snapshot.mode, snapshot.last_tick);
    let uncertain = uncertain_packs(snapshot);
    if !uncertain.is_empty() {
        health = health.with_reason("uncertain_write");
        health.uncertain_packs = uncertain;
    }
    // Startup is not a liveness failure: discovery, the first fleet read and the first solve may
    // legitimately take longer than one tick, and there is no other long operation in this engine.
    if boot_instant.elapsed() < HEALTH_STARTUP_GRACE
        || now.saturating_sub(booted_at)
            < i64::try_from(HEALTH_STARTUP_GRACE.as_secs()).unwrap_or(i64::MAX)
    {
        return (StatusCode::OK, health);
    }
    let bound = i64::try_from(stale_after.as_secs().max(1)).unwrap_or(i64::MAX);
    let Some(last_tick) = snapshot.last_tick else {
        let age = now.saturating_sub(booted_at);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            health.degraded("tick_stalled", booted_at, age),
        );
    };
    let age = now.saturating_sub(last_tick);
    if age > bound {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            health.degraded("tick_stalled", last_tick, age),
        );
    }
    if snapshot.configured_packs == 0 {
        return (StatusCode::OK, health);
    }
    let packs = required(snapshot);
    let stalled = stalled(&packs, now, bound, booted_at);
    let fresh = packs.len() > stalled.len()
        || packs.iter().any(|pack| {
            pack.sampled_at
                .is_some_and(|at| now.saturating_sub(at) <= bound)
        });
    if !fresh {
        // Fleet-blind: nothing required has a usable observation. This is the only pack-freshness
        // case that may restart the add-on.
        let since = packs
            .iter()
            .filter_map(|pack| pack.sampled_at)
            .max()
            .unwrap_or(booted_at);
        let age = now.saturating_sub(since);
        let mut result = health.degraded("sampler_stalled", since, age);
        result.stalled_packs = stalled;
        return (StatusCode::SERVICE_UNAVAILABLE, result);
    }
    if !stalled.is_empty() {
        health.degraded = true;
        health.stalled_packs = stalled;
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
mod tests;

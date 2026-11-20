//! The running process: the API, the engine task, the stores and the background loops.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::Router;
use chrono::Utc;
use energy_core::engine::{Engine, HoldReport, PACK_HOLD_BUDGET};
use energy_ledger::Ledger;
use tokio::task::JoinHandle;

use crate::app;
use crate::bootstrap::site_cache::SiteCache;
use crate::service::Service;

/// One absolute deadline for the whole stop: graceful engine hold, fallback, then final writes.
const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(20);
/// The engine's own hold gets one full per-pack budget; fan-out makes that a fleet-wide cost.
const GRACEFUL_SLICE: Duration = PACK_HOLD_BUDGET;
/// A stuck engine gets only the bounded remainder, never a second full deadline.
const FALLBACK_SLICE: Duration = Duration::from_secs(4);
/// Cancellation, per-pack journaling and the WAL checkpoint still need their own time.
const FINAL_RESERVE: Duration = Duration::from_secs(3);
const _: () = assert!(
    GRACEFUL_SLICE.as_secs() + FALLBACK_SLICE.as_secs() + FINAL_RESERVE.as_secs()
        < SHUTDOWN_DEADLINE.as_secs()
);

type EngineTask = JoinHandle<std::result::Result<(), energy_core::engine::EngineError>>;

/// A running process: the API, the engine task, the stores and the background loops.
pub struct Booted {
    /// The HTTP read model and command queue.
    pub service: Arc<Service>,
    /// The control engine (already booted).
    pub engine: Arc<Engine>,
    /// The ledger (one writer, checkpointed on the way out).
    pub ledger: Arc<Ledger>,
    /// Discovery and the server-side faults.
    pub cache: Arc<SiteCache>,
    pub(crate) address: SocketAddr,
    pub(crate) engine_task: Option<EngineTask>,
    pub(crate) tasks: Vec<JoinHandle<()>>,
}

impl Booted {
    /// The router, for tests and for [`run`].
    pub fn router(&self) -> Router {
        app::router(Arc::clone(&self.service))
    }

    /// The bound (or to-be-bound) ingress address.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Stop everything and checkpoint, as `SIGTERM` would.
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(task) = self.engine_task.take() {
            task.abort();
            let _ = task.await;
        }
        let report = self
            .engine
            .stop(Utc::now().timestamp(), "shutdown")
            .await
            .context("the engine could not hold its packs")?;
        Self::report_holds(&report);
        self.abort_tasks();
        self.ledger.checkpoint().context("checkpoint the ledger")?;
        Ok(())
    }

    /// The production stop path: the engine already saw the same signal, so wait for its hold.
    pub async fn finish(mut self) -> Result<()> {
        self.stop_engine().await?;
        self.abort_tasks();
        self.ledger.checkpoint().context("checkpoint the ledger")?;
        Ok(())
    }

    /// Wait within one absolute deadline, then hand the remaining bounded time to the fallback.
    async fn stop_engine(&mut self) -> Result<()> {
        let Some(mut task) = self.engine_task.take() else {
            return Ok(());
        };
        let deadline = Instant::now() + SHUTDOWN_DEADLINE;
        if self.wait_for_engine(&mut task, deadline).await {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let budget = remaining.saturating_sub(FINAL_RESERVE).min(FALLBACK_SLICE);
        self.force_stop(&mut task, budget).await
    }

    /// The graceful slice ends early enough to leave a fallback and a final reserve.
    async fn wait_for_engine(&self, task: &mut EngineTask, deadline: Instant) -> bool {
        // The deadline is 20 s out and these slices are compile-time bounded well below it.
        let graceful_end = deadline
            .checked_sub(FALLBACK_SLICE + FINAL_RESERVE)
            .unwrap_or(deadline);
        let wait = graceful_end
            .saturating_duration_since(Instant::now())
            .min(GRACEFUL_SLICE);
        match tokio::time::timeout(wait, task).await {
            Ok(joined) => {
                let outcome = joined
                    .map_err(|error| error.to_string())
                    .and_then(|ended| ended.map_err(|error| error.to_string()));
                tracing::info!(error = outcome.err(), "engine task ended");
                true
            }
            Err(_) => false,
        }
    }

    /// The stall path: abort the loop and spend only the remaining bounded slice on holds.
    async fn force_stop(&mut self, task: &mut EngineTask, budget: Duration) -> Result<()> {
        tracing::warn!(
            graceful_s = GRACEFUL_SLICE.as_secs(),
            fallback_s = budget.as_secs(),
            reserve_s = FINAL_RESERVE.as_secs(),
            "engine task missed its slice; retrying bounded verified holds"
        );
        task.abort();
        self.retry_holds("shutdown timeout", budget).await
    }

    /// The last-resort holds use the same fenced, verified core path as the engine's own stop.
    async fn retry_holds(&self, reason: &str, budget: Duration) -> Result<()> {
        let report = self
            .engine
            .stop_within(Utc::now().timestamp(), reason, budget)
            .await
            .context("the engine could not hold its packs")?;
        Self::report_holds(&report);
        Ok(())
    }

    /// One line per pack the engine may have left driving, never one aggregate guess.
    fn report_holds(report: &HoldReport) {
        Self::report_group(report.unverified(), "unverified");
        Self::report_group(report.fenced(), "fenced");
    }

    fn report_group(packs: &[String], outcome: &str) {
        for pack in packs {
            tracing::warn!(
                pack = pack.as_str(),
                outcome,
                "shutdown hold needs attention"
            );
        }
    }

    fn abort_tasks(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_slices_fit_one_absolute_deadline() {
        assert!(
            GRACEFUL_SLICE + FALLBACK_SLICE + FINAL_RESERVE < SHUTDOWN_DEADLINE,
            "graceful + fallback + reserve must leave scheduling slack"
        );
        assert_eq!(FALLBACK_SLICE + FINAL_RESERVE, Duration::from_secs(7));
    }
}

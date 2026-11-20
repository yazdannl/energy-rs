//! The running process: the API, the engine task, the stores and the background loops.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::Router;
use chrono::Utc;
use energy_core::engine::{Engine, EngineError, PACK_HOLD_BUDGET};
use energy_ledger::Ledger;
use tokio::task::JoinHandle;

use crate::app;
use crate::bootstrap::site_cache::SiteCache;
use crate::service::Service;

/// How long the shutdown path waits for the engine's own SIGTERM hold before retrying it.
const ENGINE_STOP_TIMEOUT: Duration = Duration::from_secs(20);

/// All holds are fanned out, so the whole fleet's bounded wait must still fit under the timeout.
const _: () = assert!(ENGINE_STOP_TIMEOUT.as_secs() > PACK_HOLD_BUDGET.as_secs());

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
    pub(crate) engine_task:
        Option<JoinHandle<std::result::Result<(), energy_core::engine::EngineError>>>,
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
        self.engine
            .stop(Utc::now().timestamp(), "shutdown")
            .await
            .context("the engine could not hold its packs")?;
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

    /// Wait for the engine's own signal hold, or hold directly when it is stuck.
    async fn stop_engine(&mut self) -> Result<()> {
        let Some(mut task) = self.engine_task.take() else {
            return Ok(());
        };
        match tokio::time::timeout(ENGINE_STOP_TIMEOUT, &mut task).await {
            Ok(joined) => {
                let outcome = joined
                    .map_err(|error| error.to_string())
                    .and_then(|ended| ended.map_err(|error| error.to_string()));
                tracing::info!(error = outcome.err(), "engine task ended");
                Ok(())
            }
            Err(_) => self.force_stop(&mut task).await,
        }
    }

    /// The stall path: abort the loop and hold the packs directly.
    async fn force_stop(
        &mut self,
        task: &mut tokio::task::JoinHandle<
            std::result::Result<(), energy_core::engine::EngineError>,
        >,
    ) -> Result<()> {
        tracing::warn!(
            timeout_s = ENGINE_STOP_TIMEOUT.as_secs(),
            budget_s = PACK_HOLD_BUDGET.as_secs(),
            "engine task did not stop; retrying bounded verified holds"
        );
        task.abort();
        self.retry_holds("shutdown timeout").await
    }

    /// The last-resort holds use the same bounded, verified core path as the engine's own stop.
    async fn retry_holds(&self, reason: &str) -> Result<()> {
        let outcome = self.engine.stop(Utc::now().timestamp(), reason).await;
        Self::report_unverified(&outcome);
        outcome.context("the engine could not hold its packs")
    }

    /// One line per pack whose hold was not verified, never one aggregate guess.
    fn report_unverified(outcome: &Result<(), EngineError>) {
        if let Err(EngineError::Holds { detail, .. }) = outcome {
            for pack in detail.split(';') {
                tracing::warn!(pack = pack.trim(), "shutdown hold unverified");
            }
        }
    }

    fn abort_tasks(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
    }
}

//! The tokio loop: one ticker, one guard, and a SIGTERM/SIGINT that always holds.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;

use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::stream::EngineCommand;
use tokio::sync::mpsc;

impl Engine {
    /// Boot, then run the tick loop and the guard loop until a termination signal arrives.
    ///
    /// The guard runs independently of the tick loop on purpose: a tick stuck in a device read must
    /// not also stop the watchdog from noticing. Both share the engine's state mutex, which is only
    /// ever held for synchronous work.
    pub async fn run(self: &Arc<Self>) -> Result<(), EngineError> {
        self.run_with(None).await
    }

    /// Run with the API's command queue: commands are applied between ticks.
    pub async fn run_with(
        self: &Arc<Self>,
        commands: Option<mpsc::Receiver<EngineCommand>>,
    ) -> Result<(), EngineError> {
        if !self.lock()?.booted {
            self.boot(Utc::now().timestamp()).await?;
        }
        let interval = Duration::from_secs(self.config.tick_seconds.max(1));
        let guard = Arc::clone(self);
        let guard_task = tokio::spawn(async move { guard.guard_loop(interval).await });
        let running = self.tick_loop(interval, commands).await;
        guard_task.abort();
        self.shutdown(Utc::now().timestamp(), "terminated").await?;
        running
    }

    /// The tick loop, until `SIGTERM`/`SIGINT`.
    async fn tick_loop(
        &self,
        interval: Duration,
        mut commands: Option<mpsc::Receiver<EngineCommand>>,
    ) -> Result<(), EngineError> {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let command = next_command(&mut commands);
            tokio::select! {
                () = terminate() => return Ok(()),
                _ = tokio::signal::ctrl_c() => return Ok(()),
                Some(command) = command => self.command_or_log(command).await,
                _ = ticker.tick() => self.tick_or_log().await,
            }
        }
    }

    /// Apply one API command, journalling a failure instead of ending the loop.
    async fn command_or_log(&self, command: EngineCommand) {
        let now = Utc::now().timestamp();
        let kind = command.kind();
        if let Err(error) = self.apply_command(command, now).await {
            tracing::error!(%error, command = kind, "engine command failed");
            let _ = self.event(
                now,
                energy_ledger::EventLevel::Error,
                "engine.command_failed",
                format!("{kind}: {error}"),
            );
        }
    }

    /// One tick, with its failure boundary instead of ending the loop.
    ///
    /// A failed tick journals the cause and attempts holds for the packs this process owns; it never
    /// marks progress, so a loop that keeps failing still reaches the watchdog.
    async fn tick_or_log(&self) {
        let now = Utc::now().timestamp();
        if let Err(error) = self.tick_once(now).await {
            tracing::error!("tick failed: {error}");
            self.tick_failed(now, &error).await;
        }
    }

    /// Trigger a graceful shutdown hold without a signal (the server's `/stop` and the tests).
    pub async fn stop(&self, now: i64, reason: &str) -> Result<(), EngineError> {
        self.shutdown(now, reason).await
    }

    /// The guard loop: watchdog, boot-recovery retry and, later, the maintenance publisher.
    async fn guard_loop(self: Arc<Self>, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let now = Utc::now().timestamp();
            if let Err(error) = self.supervise(now).await {
                tracing::error!(%error, "guard step failed");
            }
        }
    }
}

/// The next queued command, or a future that never resolves when the server has no queue.
async fn next_command(
    commands: &mut Option<mpsc::Receiver<EngineCommand>>,
) -> Option<EngineCommand> {
    match commands.as_mut() {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending().await,
    }
}

/// Resolve when the process is asked to terminate.
///
/// `SIGTERM` is the add-on supervisor's normal stop signal, so it must produce the same hold as a
/// clean `Ctrl-C`. A missing signal registration is not fatal: the branch simply never resolves and
/// the tick loop keeps running.
async fn terminate() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut stream) = signal(SignalKind::terminate()) {
            stream.recv().await;
            return;
        }
    }
    std::future::pending::<()>().await;
}

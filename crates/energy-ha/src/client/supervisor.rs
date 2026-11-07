//! Reconnecting WebSocket supervisor.

use std::{sync::Arc, time::Duration};

use tokio::{
    sync::watch,
    time::{sleep, timeout},
};
use tracing::warn;

use super::{Config, Connection, HaError, LinkState};

/// Keeps one authenticated WebSocket connection alive and reconnects with backoff.
///
/// Consumers watch [`Supervisor::connection`] (or wait via [`Supervisor::connection_within`]) and
/// re-subscribe when the connection object changes.
pub struct Supervisor {
    connection: watch::Receiver<Option<Arc<Connection>>>,
    state: watch::Receiver<LinkState>,
    shutdown: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl Supervisor {
    pub fn spawn(config: Config) -> Self {
        let (connection_tx, connection_rx) = watch::channel::<Option<Arc<Connection>>>(None);
        let (state_tx, state_rx) = watch::channel(LinkState::Connecting);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(run(config, connection_tx, state_tx, shutdown_rx));
        Self {
            connection: connection_rx,
            state: state_rx,
            shutdown: shutdown_tx,
            task,
        }
    }

    pub fn connection(&self) -> watch::Receiver<Option<Arc<Connection>>> {
        self.connection.clone()
    }

    pub fn state(&self) -> watch::Receiver<LinkState> {
        self.state.clone()
    }

    /// Wait until a connection is live.
    pub async fn connection_within(&self, wait: Duration) -> Result<Arc<Connection>, HaError> {
        if let Some(connection) = self.connection.borrow().clone() {
            return Ok(connection);
        }
        let mut receiver = self.connection.clone();
        let wait_for_connection = async {
            loop {
                if receiver.changed().await.is_err() {
                    return Err(HaError::Disconnected);
                }
                if let Some(connection) = receiver.borrow().clone() {
                    return Ok(connection);
                }
            }
        };
        timeout(wait, wait_for_connection)
            .await
            .map_err(|_| HaError::Timeout(wait))?
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        self.task.abort();
    }
}

/// Connect, or `None` when shutdown won the race.
async fn connect_or_shutdown(
    config: &Config,
    shutdown: &mut watch::Receiver<bool>,
) -> Option<Result<Connection, HaError>> {
    let connect = Connection::connect(config);
    tokio::pin!(connect);
    tokio::select! {
        _ = shutdown.changed() => None,
        result = &mut connect => Some(result),
    }
}

/// Wait until the connection drops; `true` when shutdown won the race.
async fn wait_for_close(connection: &Connection, shutdown: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        () = connection.closed() => false,
        _ = shutdown.changed() => true,
    }
}

/// Sleep one backoff step; `true` when shutdown won the race.
async fn sleep_or_shutdown(delay: Duration, shutdown: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = shutdown.changed() => true,
        () = sleep(delay) => false,
    }
}

async fn run(
    config: Config,
    connection_tx: watch::Sender<Option<Arc<Connection>>>,
    state_tx: watch::Sender<LinkState>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut attempt = 0u32;
    while !shutdown_loop(
        &config,
        &connection_tx,
        &state_tx,
        &mut shutdown,
        &mut attempt,
    )
    .await
    {}
    let _ = connection_tx.send(None);
    let _ = state_tx.send(LinkState::Disconnected);
}

/// One connect/backoff cycle; `true` means stop supervising.
async fn shutdown_loop(
    config: &Config,
    connection_tx: &watch::Sender<Option<Arc<Connection>>>,
    state_tx: &watch::Sender<LinkState>,
    shutdown: &mut watch::Receiver<bool>,
    attempt: &mut u32,
) -> bool {
    let _ = state_tx.send(LinkState::Connecting);
    match connect_or_shutdown(config, shutdown).await {
        None => return true,
        Some(Err(error)) => {
            warn!(error = %error, "Home Assistant WebSocket connection failed");
            let _ = connection_tx.send(None);
            let _ = state_tx.send(LinkState::Disconnected);
        }
        Some(Ok(connection)) => {
            *attempt = 0;
            let connection = Arc::new(connection);
            let _ = connection_tx.send(Some(Arc::clone(&connection)));
            let _ = state_tx.send(LinkState::Connected);
            let stop = wait_for_close(&connection, shutdown).await;
            let _ = connection_tx.send(None);
            let _ = state_tx.send(LinkState::Disconnected);
            if stop {
                return true;
            }
        }
    }
    let delay = config.reconnect.delay(*attempt);
    *attempt = attempt.saturating_add(1);
    sleep_or_shutdown(delay, shutdown).await
}

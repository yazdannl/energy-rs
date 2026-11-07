//! `energy-server` binary: the add-on entrypoint.
//!
//! Binds `0.0.0.0:8099` (ingress; never loopback only, the Supervisor connects from the Docker
//! network), logs to stdout, and shuts down gracefully on SIGTERM so the ledger can be checkpointed
//! once P6 lands. Options come from `/data/options.json`; P0 reads only the port from the
//! environment.

use std::net::{Ipv4Addr, SocketAddr};

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

/// Ingress port from `energy_rs/config.yaml`.
const DEFAULT_PORT: u16 = 8099;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let port = std::env::var("INGRESS_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        %addr,
        "energy-server listening"
    );

    axum::serve(listener, energy_server::app::router())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("http server failed")?;

    tracing::info!("energy-server stopped");
    Ok(())
}

fn init_tracing() {
    let directives = std::env::var("ENERGY_RS_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| "info".to_owned());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(directives))
        .init();
}

/// Wait for SIGTERM (Supervisor stop) or Ctrl-C.
async fn shutdown_signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                tracing::warn!(%err, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received");
}

//! `energy-server` binary: the add-on entrypoint.
//!
//! Binds `0.0.0.0:8099` (ingress; never loopback only, the Supervisor connects from the Docker
//! network), logs to stdout, and shuts down gracefully on SIGTERM so the ledger can be checkpointed.
//! Configuration comes from `/data/options.json` (written by the Supervisor); `energy_rs/run.sh`
//! passes the log level as `LOG_LEVEL` and the data folder as `ENERGY_RS_DATA`.
//!
//! The engine loop (P8b) is not wired yet: the server opens the stores, serves the read API and the
//! UI, and refuses control commands with `503 engine_unavailable` instead of pretending to accept
//! them.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use energy_ledger::{Ledger, LedgerConfig};
use energy_price::{Contract, Settlement, SqlitePriceRepository};
use energy_server::{Service, ServiceOptions, app};

/// Ingress port from `energy_rs/config.yaml`.
const DEFAULT_PORT: u16 = 8099;
/// The data volume; both stores share this one SQLite file.
const DEFAULT_DATA_DIR: &str = "/data";

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let port = std::env::var("INGRESS_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));

    let service = build_service()?;

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        %addr,
        data_dir = %std::env::var("ENERGY_RS_DATA").as_deref().unwrap_or(DEFAULT_DATA_DIR),
        "energy-server listening"
    );

    axum::serve(listener, app::router(service))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("http server failed")?;

    tracing::info!("energy-server stopped");
    Ok(())
}

/// Open the ledger and the price store and build the read model.
fn build_service() -> Result<Arc<Service>> {
    let data_dir = PathBuf::from(
        std::env::var("ENERGY_RS_DATA").unwrap_or_else(|_| DEFAULT_DATA_DIR.to_owned()),
    );
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("create data folder {}", data_dir.display()))?;
    let db_path = data_dir.join("energy_rs.db");
    let ledger = Ledger::open(&LedgerConfig::at(db_path.clone())).context("open the ledger")?;
    let prices = SqlitePriceRepository::open(&db_path).context("open the price store")?;
    let (service, handles) = Service::new(
        Arc::new(ledger),
        Arc::new(prices),
        Settlement::new(Contract::zonneplan_2026()),
        ServiceOptions::default(),
    );
    // P8b will drain this queue in the engine loop. Until then the API refuses write commands with
    // `503 engine_unavailable` instead of accepting them and silently dropping them.
    drop(handles);
    tracing::warn!("engine loop is not wired yet (P8b): control commands answer 503");
    Ok(service)
}

fn init_tracing() {
    // `LOG_LEVEL` comes from the add-on options (`log_level`) via run.sh; the other two are for
    // development and for a manual `docker run`.
    let directives = std::env::var("LOG_LEVEL")
        .or_else(|_| std::env::var("ENERGY_RS_LOG"))
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

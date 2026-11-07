//! Startup assembly and the process lifecycle.
//!
//! [`boot`] reads the add-on options, opens the stores, resolves Home Assistant, discovers the
//! batteries, builds the engine and starts the background loops. It fails fast, with a message that
//! names the missing piece, before anything serves HTTP. [`run`] adds the ingress listener and the
//! SIGTERM/SIGINT path: stop accepting, let the engine hold, checkpoint the ledger, exit 0.
//!
//! Everything the engine needs is injected here (`Options`), so an end-to-end test can boot the whole
//! process against a temporary `/data`, an in-process Home Assistant and fake batteries.

pub mod booted;
pub mod config;
pub mod engine;
pub mod fleet;
pub mod ha;
pub mod ha_loop;
pub mod logging;
pub mod meter;
mod option_value;
pub mod options;
pub mod prices;
pub mod profile;
pub mod savings;
pub mod shutdown;
pub mod site_cache;
pub mod startup;

use anyhow::{Context, Result};
use tokio::net::TcpListener;

pub use crate::bootstrap::booted::Booted;
pub use crate::bootstrap::startup::boot;

/// The add-on entrypoint: options, tracing, HTTP, and the graceful stop path.
pub async fn run() -> Result<()> {
    let options = start()?;
    let booted = boot(options).await?;
    serve(booted).await?.finish().await
}

/// Read the options and start logging before anything else can fail.
fn start() -> Result<self::options::Options> {
    let options = self::options::Options::load()?;
    logging::init(options.log_level());
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        port = options.address().port(),
        mode = options.mode().slug(),
        "energy-server starting"
    );
    Ok(options)
}

/// Serve until the Supervisor's stop signal, then hand the process back to the caller.
async fn serve(booted: Booted) -> Result<Booted> {
    let listener = TcpListener::bind(booted.address())
        .await
        .with_context(|| format!("bind the ingress port {}", booted.address()))?;
    tracing::info!(address = %booted.address(), "energy-server listening");
    axum::serve(listener, booted.router())
        .with_graceful_shutdown(shutdown::signal())
        .await
        .context("the HTTP server failed")?;
    Ok(booted)
}

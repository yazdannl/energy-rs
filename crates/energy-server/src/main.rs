//! `energy-server` binary: the Home Assistant add-on entrypoint.
//!
//! All startup work lives in [`energy_server::bootstrap`]; this file only converts the result into a
//! process exit code. `energy_rs/run.sh` `exec`s this binary, so the Supervisor's SIGTERM reaches the
//! shutdown path directly (stop accepting, hold, checkpoint the ledger, exit 0).

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    energy_server::bootstrap::run().await
}

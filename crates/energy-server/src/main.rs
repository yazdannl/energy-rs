//! `energy-server` binary: the Home Assistant add-on entrypoint, plus read-only diagnostics.
//!
//! With no subcommand this is exactly the add-on: all startup work lives in
//! [`energy_server::bootstrap`]; this file only converts the result into a process exit code.
//! `energy_rs/run.sh` `exec`s this binary, so the Supervisor's SIGTERM reaches the shutdown path
//! directly (stop accepting, hold, checkpoint the ledger, exit 0).
//!
//! `doctor` and `plan-once` are diagnostics: read-only, exit code 0/1/2 (doctor) or 0/2
//! (plan-once), no writes to Home Assistant or the batteries.

use anyhow::Result;
use clap::Parser;

use energy_server::diag::{self, Cli};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        return energy_server::bootstrap::run().await;
    };
    let code = diag::run(command, cli.verbose).await?;
    std::process::exit(code);
}

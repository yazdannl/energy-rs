//! The command line: no subcommand keeps the add-on behaviour, subcommands are diagnostics.
//!
//! `energy-server` with no arguments is the Home Assistant add-on entrypoint. `doctor` and
//! `plan-once` are read-only tools for validating an install; both accept the same environment and
//! options file as the runtime, and both refuse to write to a device.

use clap::Parser;

use crate::diag::cli_command::CliCommand;

/// `energy-server [--verbose] [doctor|plan-once]`.
#[derive(Debug, Parser)]
#[command(
    name = "energy-server",
    version,
    about = "Energy RS add-on entrypoint and read-only diagnostics",
    long_about = None
)]
pub struct Cli {
    /// Print per-phase timings in addition to the checks.
    #[arg(long, global = true)]
    pub verbose: bool,
    /// Run a diagnostic instead of the add-on.
    #[command(subcommand)]
    pub command: Option<CliCommand>,
}

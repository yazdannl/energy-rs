//! Flags shared by the two diagnostics.

use std::path::PathBuf;

use clap::Args;

/// `energy-server plan-once [--data-dir <dir>] [--json]`.
#[derive(Debug, Clone, Args)]
pub struct PlanOnceArgs {
    /// Add-on data folder (default: `$ENERGY_RS_DATA`, else `/data`).
    #[arg(long, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,
    /// Machine-readable JSON instead of the table.
    #[arg(long)]
    pub json: bool,
}

//! The diagnostic subcommands.

use clap::Subcommand;

use crate::diag::doctor_args::DoctorArgs;
use crate::diag::plan_once_args::PlanOnceArgs;

/// One read-only diagnostic.
#[derive(Debug, Subcommand)]
pub enum CliCommand {
    /// Read-only install checks: PASS/WARN/FAIL table and exit code 0/1/2.
    Doctor(DoctorArgs),
    /// Build one plan with the real plan source and print it; never writes to a device.
    PlanOnce(PlanOnceArgs),
}

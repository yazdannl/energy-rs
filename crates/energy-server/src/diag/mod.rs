//! Read-only diagnostics: `doctor` for an install check, `plan-once` for one real plan.
//!
//! Both commands reuse the runtime's bootstrap (options, HA transport, discovery, fleet, plan
//! source) and add only orchestration and rendering. Neither can reach a device write: the battery
//! rows come from `GET /properties/report`, and `plan-once` never constructs a controller command.
//! Tokens are resolved by [`Options`](crate::bootstrap::options::Options), which redacts them from
//! `Debug`, and every error they reach comes from `energy-ha`'s redacting transport.

pub mod cli;
pub mod cli_command;
pub mod doctor_args;
pub mod plan_once_args;

mod check;
mod checks;
mod doctor;
mod ha_client;
mod pack_states;
mod plan_once;
mod plan_probe;
mod plan_table;
mod report;
mod status;
mod table;

use std::path::PathBuf;

use anyhow::Result;
use serde_json::json;

use crate::bootstrap::options::Options;

pub use check::Check;
pub use cli::Cli;
pub use cli_command::CliCommand as Command;
pub use plan_probe::PlanProbe;
pub use report::Report;
pub use status::Status;

/// The doctor over explicit options (the CLI and the tests share this entry point).
pub async fn doctor_report(options: Options, verbose: bool) -> Report {
    doctor::run(options, verbose).await
}

/// One plan over explicit options, with its phase timings.
pub async fn plan_attempt(options: &Options) -> (PlanProbe, Vec<(String, u64)>) {
    plan_once::run(options).await
}

/// Run one diagnostic and print it; the returned code is the process exit code.
pub async fn run(command: Command, verbose: bool) -> Result<i32> {
    match command {
        Command::Doctor(args) => {
            let options = options(args.data_dir)?;
            let report = doctor::run(options, verbose).await;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", report.render());
            }
            Ok(report.exit_code())
        }
        Command::PlanOnce(args) => {
            let options = options(args.data_dir)?;
            let (probe, timings) = plan_once::run(&options).await;
            print_plan(&probe, args.json, verbose.then_some(timings.as_slice()))?;
            Ok(if probe.plan.is_ok() { 0 } else { 2 })
        }
    }
}

fn options(data_dir: Option<PathBuf>) -> Result<Options> {
    match data_dir {
        Some(dir) => Options::at(dir),
        None => Options::load(),
    }
}

fn print_plan(
    probe: &PlanProbe,
    json_output: bool,
    timings: Option<&[(String, u64)]>,
) -> Result<()> {
    let timings = timings.unwrap_or_default();
    if json_output {
        let mut value = match &probe.plan {
            Ok(plan) => plan_table::to_json(plan),
            Err(error) => json!({
                "refused": error.to_string(),
                "packs": probe.packs,
                "profileDays": probe.profile_days,
                "notes": probe.notes,
            }),
        };
        if !timings.is_empty()
            && let Some(object) = value.as_object_mut()
        {
            object.insert("timings".to_owned(), json!(timings));
        }
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    match &probe.plan {
        Ok(plan) => print!("{}", plan_table::render(plan)),
        Err(error) => println!("plan refused: {error}"),
    }
    if probe.packs == 0 {
        println!("packs: none with a usable state of charge");
    }
    for note in &probe.notes {
        println!("note: {note}");
    }
    if !timings.is_empty() {
        let rendered: Vec<String> = timings
            .iter()
            .map(|(phase, millis)| format!("{phase} {millis} ms"))
            .collect();
        println!("timings: {}", rendered.join(", "));
    }
    Ok(())
}

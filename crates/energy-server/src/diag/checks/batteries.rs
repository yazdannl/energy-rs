//! Battery discovery and per-device report fields, read with `GET /properties/report` only.
//!
//! Nothing here can write: the doctor builds the same fleet the runtime builds (manual entries plus
//! mDNS) but only calls the fleet's read path, then renders each report. A wedged `acMode`, a
//! missing pack label and a stale device clock are visible without touching a device.

use energy_battery::control::ControlLimits;
use energy_battery::fleet::{DeviceSnapshot, FleetSnapshot};
use energy_battery::report::Report;

use crate::bootstrap::fleet;
use crate::bootstrap::options::Options;
use crate::diag::check::Check;
use crate::diag::status::Status;

/// Discovery summary plus one row per device; the snapshots feed the plan probe's SoC.
pub async fn checks(options: &Options, now: i64) -> (Vec<Check>, Vec<DeviceSnapshot>) {
    match read(options).await {
        Ok((snapshot, manual, discovered)) => {
            let mut checks = vec![summary(&snapshot, manual, discovered)];
            for device in &snapshot.devices {
                checks.push(device_check(device, now));
            }
            (checks, snapshot.devices)
        }
        Err(error) => (vec![Check::fail("batteries", error)], Vec::new()),
    }
}

/// The read side only: the device snapshots, for a plan attempt that does not need a table.
pub async fn snapshots(options: &Options) -> Vec<DeviceSnapshot> {
    read(options)
        .await
        .map(|(snapshot, _, _)| snapshot.devices)
        .unwrap_or_default()
}

async fn read(options: &Options) -> Result<(FleetSnapshot, usize, usize), String> {
    let fleet = fleet::build(options, ControlLimits::unlimited())
        .await
        .map_err(|error| error.to_string())?;
    let snapshot = fleet.client.refresh().await;
    Ok((snapshot, fleet.manual, fleet.discovered))
}

fn summary(snapshot: &FleetSnapshot, manual: usize, discovered: usize) -> Check {
    let total = snapshot.devices.len();
    let reachable = snapshot
        .devices
        .iter()
        .filter(|device| device.is_online())
        .count();
    let detail = format!(
        "manual {manual}, mDNS {discovered}, devices {total}, reachable {reachable}/{total}",
    );
    if total == 0 {
        return Check::fail(
            "batteries",
            format!("{detail}; add address/serial entries or enable mDNS"),
        );
    }
    if reachable < total {
        return Check::warn("batteries", detail);
    }
    Check::pass("batteries", detail)
}

fn device_check(device: &DeviceSnapshot, now: i64) -> Check {
    let name = format!("battery {}", device.sn);
    if let Some(error) = &device.error {
        return Check::fail(name, format!("unreachable: {error}"));
    }
    let Some(report) = &device.report else {
        return Check::fail(name, "no report");
    };
    let mut notes = vec![
        report
            .product
            .clone()
            .unwrap_or_else(|| "product unknown".to_owned()),
    ];
    let mut status = Status::Pass;
    identity(device, report, &mut notes, &mut status);
    packs(report, &mut notes, &mut status);
    limits(report, &mut notes);
    live(report, now, &mut notes, &mut status);
    check(name, status, &notes)
}

fn identity(
    device: &DeviceSnapshot,
    report: &Report,
    notes: &mut Vec<String>,
    status: &mut Status,
) {
    if report.sn.eq_ignore_ascii_case(&device.sn) {
        notes.push("SN match".to_owned());
    } else {
        notes.push(format!(
            "SN mismatch (configured {}, device {})",
            device.sn, report.sn
        ));
        *status = Status::Warn;
    }
}

fn packs(report: &Report, notes: &mut Vec<String>, status: &mut Status) {
    let labels: Vec<String> = report
        .packs
        .iter()
        .map(|pack| {
            pack.label.map_or_else(
                || format!("{} unknown", pack.sn),
                |label| format!("{} {} kWh", label.model, label.capacity_kwh),
            )
        })
        .collect();
    if labels.is_empty() {
        notes.push("no packData".to_owned());
        *status = Status::Warn;
        return;
    }
    notes.push(format!("packs {}", labels.join(" + ")));
    if report.total_capacity_kwh().is_none() {
        notes.push("no labelled capacity".to_owned());
        *status = Status::Warn;
    }
}

fn limits(report: &Report, notes: &mut Vec<String>) {
    let properties = &report.properties;
    notes.push(format!(
        "chargeMaxLimit {} W / inverseMaxPower {} W",
        properties.charge_max_w, properties.inverse_max_w
    ));
}

fn live(report: &Report, now: i64, notes: &mut Vec<String>, status: &mut Status) {
    let properties = &report.properties;
    notes.push(format!("soc {:.1}%", report.soc_pct()));
    notes.push(format!("ac {} W", report.ac_power_w()));
    let echo_age = now.saturating_sub(report.timestamp);
    notes.push(format!("echo age {}s", echo_age.max(0)));
    if !(0..=3_600).contains(&echo_age) {
        notes.push("device clock skewed".to_owned());
        *status = Status::Warn;
    }
    match properties.ac_mode {
        0..=2 => notes.push(format!("acMode {} ok", properties.ac_mode)),
        wedged => {
            notes.push(format!("acMode {wedged}: AC path wedged"));
            *status = Status::Fail;
        }
    }
    if properties.is_error {
        notes.push("is_error is set".to_owned());
        *status = Status::Fail;
    } else if let Some(level) = properties.fault_level.filter(|level| *level >= 2) {
        notes.push(format!("faultLevel {level}"));
        *status = Status::Warn;
    }
}

fn check(name: String, status: Status, notes: &[String]) -> Check {
    let detail = notes.join(", ");
    match status {
        Status::Pass => Check::pass(name, detail),
        Status::Warn => Check::warn(name, detail),
        Status::Fail => Check::fail(name, detail),
    }
}

//! Doctor and plan-once: the table, the exit codes, redaction, and an in-process end-to-end run.
//!
//! The end-to-end tests boot the real diagnostics over the real `energy-ha` client, the real fleet
//! and the real plan source, against a hermetic fake Home Assistant and fake devices. Both commands
//! are asserted to leave the devices untouched (no `POST /properties/write`, no mode change).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use chrono::Utc;
use clap::Parser;
use energy_battery::fake::{FakeDevice, FakePack, FakeState};
use energy_ledger::{Ledger, LedgerConfig, Pack, Plan, Sample};
use energy_price::{PriceBook, PriceRepository, Slot, SlotSource, SqlitePriceRepository};
use energy_server::diag::{Check, Cli, Command, Report, Status};
use support::fake_ha::{FakeHa, TOKEN};
use tempfile::TempDir;

use energy_server::bootstrap::options::Options;

/// A serial the pack-label table reads as an AB2000 (`C`, fourth char not `E`/`F`).
const SERIAL: &str = "C04A000000000001";

#[test]
fn table_formatting_and_exit_codes() {
    let mut report = Report::default();
    assert_eq!(report.exit_code(), 0);

    report.push(Check::warn(
        "stats solar 5min/24h",
        "97.0% (280/288), unit kWh (energy)",
    ));
    assert_eq!(report.exit_code(), 1);
    let text = report.render();
    assert!(text.starts_with("STATUS  CHECK"), "{text}");
    assert!(text.contains("WARN    stats solar 5min/24h"), "{text}");
    assert!(!text.contains("next steps:"), "warnings are not failures");

    report.push(Check::fail(
        "ha transport",
        "direct token, GET /api/config failed: HTTP 401",
    ));
    assert_eq!(report.exit_code(), 2);
    let text = report.render();
    assert!(
        text.contains("2 check(s): 0 pass, 1 warn, 1 fail"),
        "{text}"
    );
    assert!(text.contains("next steps:"), "{text}");
    assert!(
        text.contains("check the Home Assistant URL/token"),
        "{text}"
    );
    assert!(text.lines().all(|line| !line.ends_with(' ')), "{text}");
}

#[test]
fn the_cli_keeps_the_add_on_when_no_subcommand_is_given() {
    let add_on = Cli::parse_from(["energy-server"]);
    assert!(add_on.command.is_none());
    let doctor = Cli::parse_from(["energy-server", "doctor", "--json"]);
    assert!(matches!(doctor.command, Some(Command::Doctor(_))));
    let plan = Cli::parse_from(["energy-server", "plan-once", "--verbose"]);
    assert!(plan.verbose);
    assert!(matches!(plan.command, Some(Command::PlanOnce(_))));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn doctor_checks_a_fake_install_without_writing_a_device() {
    let ha = FakeHa::spawn().await;
    let state = FakeState::new(SERIAL)
        .with_soc(55.0)
        .with_packs(vec![FakePack::ab2000(SERIAL)]);
    let device = FakeDevice::spawn(state).await.expect("the fake device");
    device
        .with_state(|state| state.timestamp = Utc::now().timestamp())
        .expect("the device clock is now");
    let dir = tempfile::tempdir().expect("a temp dir");
    seed(&dir, &[SERIAL]);

    let options = Options::at(dir.path())
        .expect("the options load")
        .with_direct_ha(ha.base_url(), TOKEN)
        .with_mdns(false)
        .with_scan(false)
        .with_manual_battery(device.host(), device.serial(), device.port());

    let report = energy_server::diag::doctor_report(options, true).await;
    let text = report.render();
    assert!(!text.contains(TOKEN), "the token leaked into the table");
    let json = serde_json::to_string(&report).expect("serialisable");
    assert!(!json.contains(TOKEN), "the token leaked into the JSON");
    assert!(text.contains("ha transport"), "{text}");
    assert!(text.contains("energy dashboard"), "{text}");
    assert!(text.contains("stats grid_import 5min/24h"), "{text}");
    assert!(text.contains("live meter"), "{text}");
    assert!(text.contains("prices coverage"), "{text}");
    assert!(text.contains("ledger"), "{text}");
    assert!(text.contains("verdict"), "{text}");
    assert!(text.contains("timings:"), "verbose adds timings");

    let battery = report
        .checks
        .iter()
        .find(|check| check.name == format!("battery {SERIAL}"))
        .expect("one row per device");
    assert_eq!(battery.status, Status::Pass, "{}", battery.detail);
    assert!(battery.detail.contains("SN match"), "{}", battery.detail);
    assert!(battery.detail.contains("AB2000 1.92 kWh"));
    assert!(battery.detail.contains("chargeMaxLimit 1000 W"));

    let verdict = report
        .checks
        .iter()
        .find(|check| check.name == "verdict")
        .expect("a verdict");
    assert_ne!(verdict.status, Status::Fail, "{}", verdict.detail);
    assert!(
        verdict.detail.contains("would plan") || verdict.detail.contains("would hold"),
        "{}",
        verdict.detail
    );
    assert!(report.exit_code() <= 1, "{text}");
    assert_eq!(
        device.state().expect("the fake device state").write_count(),
        0,
        "the doctor must never write to a device"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn plan_once_solves_from_a_seeded_ledger_and_does_not_change_the_mode() {
    let ha = FakeHa::spawn().await;
    let device = FakeDevice::spawn(FakeState::new(SERIAL))
        .await
        .expect("the fake device");
    let dir = tempfile::tempdir().expect("a temp dir");
    seed(&dir, &[SERIAL, "AB2000-TWO"]);
    let options = Options::at(dir.path())
        .expect("the options load")
        .with_direct_ha(ha.base_url(), TOKEN)
        .with_mdns(false)
        .with_scan(false)
        .with_manual_battery(device.host(), device.serial(), device.port());

    let (probe, timings) = energy_server::diag::plan_attempt(&options).await;
    let plan = probe.plan.as_ref().expect("a plan from the seeded ledger");
    assert!(plan.slots.len() >= 4, "a usable horizon");
    assert_eq!(probe.packs, 2, "both ledger packs plan states");
    assert_eq!(timings.len(), 4, "devices, client, connection, plan");

    let ledger = Ledger::open(&LedgerConfig::at(dir.path().join("energy_rs.db")))
        .expect("the ledger reopens");
    let mode = ledger
        .settings()
        .expect("settings")
        .get("control.mode")
        .expect("the mode read");
    assert_eq!(mode.as_deref(), Some("dry_run"), "the mode is untouched");
    assert!(
        ledger.plans().expect("plans").current().is_ok(),
        "reads stay reads"
    );
    assert_eq!(
        device.state().expect("the fake device state").write_count(),
        0,
        "plan-once must never write to a device"
    );
}

/// A seeded ledger (two packs with a sample, a plan, a mode) and three days of prices.
fn seed(dir: &TempDir, serials: &[&str]) {
    let path = dir.path().join("energy_rs.db");
    let now = Utc::now().timestamp();
    let ledger = Ledger::open(&LedgerConfig::at(&path)).expect("the ledger opens");
    for (index, sn) in serials.iter().enumerate() {
        let pack = Pack::new(*sn, 1.92, now).with_limits(800.0, 800.0);
        let stored = ledger
            .packs()
            .expect("packs")
            .upsert(&pack)
            .expect("a pack");
        ledger
            .measurements()
            .expect("measurements")
            .write(&Sample::new(
                stored.id,
                now,
                0.0,
                Some(50.0 + 5.0 * index as f64),
            ))
            .expect("a sample");
    }
    ledger
        .settings()
        .expect("settings")
        .set("control.mode", "dry_run", now)
        .expect("the mode");
    ledger
        .plans()
        .expect("plans")
        .insert(&Plan::new(now, "dry_run", now, now + 3_600, "{}"), &[])
        .expect("a plan");
    drop(ledger);

    let base = now - now.rem_euclid(900);
    let slots = (0..288)
        .map(|index| {
            let buy = 0.20 + f64::from(index % 24) * 0.005;
            Slot::new(
                base + i64::from(index) * 900,
                buy,
                (buy - 0.02) / 1.21 - 0.091_61,
                SlotSource::ZonneplanQuarterHourly,
            )
        })
        .collect::<Vec<_>>();
    let store = SqlitePriceRepository::open(&path).expect("the price store opens");
    store
        .save_publication(&PriceBook::new(slots).expect("a contiguous book"))
        .expect("the book is stored");
}

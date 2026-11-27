//! The configured replan interval must reach the plan source: a zero lookback setting resolves to
//! it at runtime, so leaving `SourceConfig` at its 900 s default would silently widen the window.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use energy_ledger::{Ledger, LedgerConfig};
use energy_server::bootstrap::{config, engine, options::Options};

fn ledger_at(dir: &tempfile::TempDir) -> Ledger {
    Ledger::open(&LedgerConfig::at(dir.path().join("energy_rs.db"))).expect("ledger")
}

#[test]
fn stored_replan_seconds_reach_the_plan_source() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let ledger = ledger_at(&dir);
    {
        let settings = ledger.settings().expect("settings");
        settings.set("replan_seconds", "600", 0).expect("replan");
    }

    let options = Options::at(dir.path()).expect("options");
    let engine_config = config::engine_config(&options, &ledger);
    let source = engine::source_config(&engine_config, config::policy(&ledger));

    assert_eq!(engine_config.replan_seconds, 600);
    assert_eq!(
        source.replan_seconds, 600,
        "the plan source kept its default"
    );
    assert_eq!(
        engine_config.load_forecast_bias_lookback_s, 0,
        "dynamic default"
    );
}

#[test]
fn whole_number_bias_settings_keep_their_exact_runtime_values() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let ledger = ledger_at(&dir);
    {
        let settings = ledger.settings().expect("settings");
        settings
            .set("load_forecast_bias_slots", "3", 0)
            .expect("slots");
        settings
            .set("load_forecast_bias_lookback_s", "600", 0)
            .expect("lookback");
    }

    let options = Options::at(dir.path()).expect("options");
    let engine_config = config::engine_config(&options, &ledger);
    let source = engine::source_config(&engine_config, config::policy(&ledger));

    assert_eq!(source.load_forecast_bias_slots, 3);
    assert_eq!(source.load_forecast_bias_lookback_s, 600);
}

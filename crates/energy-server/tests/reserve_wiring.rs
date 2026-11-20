//! The stored reserve must reach the plan source, not just the safety ladder.
//!
//! The boot path builds `EngineConfig` from the ledger and then the plan source's `SourceConfig`.
//! This test pins the hand-off: dropping it silently plans to the pack's hardware floor while the
//! ladder refuses to deliver below the operator's reserve.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use energy_ledger::{Ledger, LedgerConfig};
use energy_server::bootstrap::{config, engine, options::Options};

#[test]
fn stored_reserve_soc_pct_is_wired_from_the_ledger_into_the_plan_source() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let ledger = Ledger::open(&LedgerConfig::at(dir.path().join("energy_rs.db"))).expect("ledger");
    {
        // A repository holds the connection guard: drop it before the config reads.
        let settings = ledger.settings().expect("settings");
        settings.set("reserve_soc_pct", "15", 0).expect("reserve");
        settings
            .set("peak_reserve_pct", "10", 0)
            .expect("peak reserve");
        settings
            .set("solar_reserve_pct", "10", 0)
            .expect("solar reserve");
    }

    let options = Options::at(dir.path()).expect("options");
    let engine_config = config::engine_config(&options, &ledger);
    let source = engine::source_config(&engine_config, config::policy(&ledger));

    assert_eq!(engine_config.reserve_soc_pct, 15.0);
    assert_eq!(
        source.reserve_soc_pct, 15.0,
        "the plan source silently kept its own default floor"
    );
    // The soft reserves keep their own settings and their own semantics.
    assert_eq!(source.policy.peak_reserve_pct, 10.0);
    assert_eq!(source.policy.solar_reserve_pct, 10.0);
}

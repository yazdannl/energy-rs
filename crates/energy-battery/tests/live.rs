//! Live hardware smoke test — read only, ignored by default.
//!
//! Run it deliberately against one device on the LAN:
//!
//! ```text
//! ENERGY_RS_LIVE_HOST=192.168.1.31 ENERGY_RS_LIVE_SN=EOC1NLN9N4XXXXXX \
//!   cargo test -p energy-battery --test live -- --ignored --nocapture
//! ```
//!
//! It only reads: `GET /properties/report` and a check that the decoded state is coherent. No
//! write is ever sent from a test, and the test skips itself when the environment is not set.

// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

use std::time::Duration;

use energy_battery::{Endpoint, HttpTransport};

fn live_target() -> Option<(String, String)> {
    let host = std::env::var("ENERGY_RS_LIVE_HOST").ok()?;
    let sn = std::env::var("ENERGY_RS_LIVE_SN").ok()?;
    if host.trim().is_empty() || sn.trim().is_empty() {
        return None;
    }
    Some((host, sn))
}

#[tokio::test]
#[ignore = "touches a real battery; set ENERGY_RS_LIVE_HOST and ENERGY_RS_LIVE_SN"]
async fn reads_a_real_device() {
    let Some((host, sn)) = live_target() else {
        eprintln!("skip: ENERGY_RS_LIVE_HOST / ENERGY_RS_LIVE_SN are not set");
        return;
    };
    let transport =
        HttpTransport::with_timeout(Endpoint::new(host, sn.clone()), Duration::from_secs(5))
            .unwrap();
    let report = transport.fetch_report().await.unwrap();

    assert_eq!(report.sn, sn, "the configured serial must match the device");
    assert!(
        (0.0..=100.0).contains(&report.soc_pct()),
        "soc {}",
        report.soc_pct()
    );
    assert!(
        matches!(report.properties.ac_mode, 1 | 2),
        "acMode {} is not a usable direction",
        report.properties.ac_mode
    );
    eprintln!(
        "{} {}: soc {:.0} %, acMode {}, input {} W, output {} W, packs {} ({:?} kWh), smartMode {}",
        report.product.as_deref().unwrap_or("unknown"),
        report.sn,
        report.soc_pct(),
        report.properties.ac_mode,
        report.properties.input_limit_w,
        report.properties.output_limit_w,
        report.packs.len(),
        report.total_capacity_kwh(),
        report.properties.smart_mode,
    );
}

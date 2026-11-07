//! `plan-once`: one plan through the real source, printed as a table or JSON. No writes, no mode
//! change, no ledger rows.

use std::time::Instant;

use chrono::Utc;

use crate::bootstrap::options::Options;
use crate::diag::checks::batteries;
use crate::diag::ha_client;
use crate::diag::plan_probe::{self, PlanProbe};

/// The plan attempt plus its phase timings (`--verbose`).
pub async fn run(options: &Options) -> (PlanProbe, Vec<(String, u64)>) {
    let now = Utc::now().timestamp();
    let mut timings = Vec::new();

    let mark = Instant::now();
    let devices = batteries::snapshots(options).await;
    timings.push(("devices".to_owned(), millis(mark)));

    let mark = Instant::now();
    let client = match ha_client::build(options) {
        Ok(client) => client,
        Err(reason) => {
            timings.push(("client".to_owned(), millis(mark)));
            return (plan_probe::refused(reason), timings);
        }
    };
    timings.push(("client".to_owned(), millis(mark)));

    let mark = Instant::now();
    let connection = match client.connection().await {
        Ok(connection) => connection,
        Err(error) => {
            timings.push(("connection".to_owned(), millis(mark)));
            return (plan_probe::refused(error.to_string()), timings);
        }
    };
    timings.push(("connection".to_owned(), millis(mark)));

    let mark = Instant::now();
    let probe = plan_probe::run(connection, options, &devices, now).await;
    timings.push(("plan".to_owned(), millis(mark)));
    (probe, timings)
}

fn millis(mark: Instant) -> u64 {
    u64::try_from(mark.elapsed().as_millis()).unwrap_or(u64::MAX)
}

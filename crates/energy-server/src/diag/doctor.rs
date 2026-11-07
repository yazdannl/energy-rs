//! The doctor: every read-only check in one report, in the order a failure should be fixed.

use std::time::Instant;

use chrono::Utc;
use energy_ha::HaClient;

use crate::bootstrap::options::Options;
use crate::diag::check::Check;
use crate::diag::checks::{batteries, ledger, live, prices, site, stats, transport, verdict};
use crate::diag::plan_probe::{self, PlanProbe};
use crate::diag::report::Report;

/// Run every check; the report's exit code is 0/1/2.
pub async fn run(options: Options, verbose: bool) -> Report {
    let now = Utc::now().timestamp();
    let mut report = Report::default();
    let mut mark = Instant::now();

    let (transport, client) = transport::check(&options).await;
    report.push(transport);
    record(&mut report, verbose, "transport", mark);

    mark = Instant::now();
    let site = dashboard(&mut report, client.as_ref()).await;
    record(&mut report, verbose, "dashboard", mark);

    mark = Instant::now();
    let (checks, devices) = batteries::checks(&options, now).await;
    report.extend(checks);
    record(&mut report, verbose, "batteries", mark);

    mark = Instant::now();
    meter(&mut report, client.as_ref(), site.as_ref(), now).await;
    record(&mut report, verbose, "statistics", mark);

    mark = Instant::now();
    report.extend(prices::checks(&options, now));
    record(&mut report, verbose, "prices", mark);

    mark = Instant::now();
    report.extend(ledger::checks(&options, now));
    record(&mut report, verbose, "ledger", mark);

    mark = Instant::now();
    let probe = attempt(client.as_ref(), site.as_ref(), &options, &devices, now).await;
    record(&mut report, verbose, "plan", mark);

    let failures = report.failures().count();
    report.push(verdict::check(failures, probe.as_ref(), now));
    report
}

/// The dashboard discovery row, or a failure when there is no client at all.
async fn dashboard(
    report: &mut Report,
    client: Option<&std::sync::Arc<HaClient>>,
) -> Option<energy_ha::SiteModel> {
    let Some(client) = client else {
        report.push(Check::fail(
            "energy dashboard",
            "not checked: Home Assistant is unreachable",
        ));
        return None;
    };
    let (check, site) = site::check(client).await;
    report.push(check);
    site
}

/// Statistics coverage and the live meter; skipped (with a warning) without a WebSocket.
async fn meter(
    report: &mut Report,
    client: Option<&std::sync::Arc<HaClient>>,
    site: Option<&energy_ha::SiteModel>,
    now: i64,
) {
    let (Some(client), Some(site)) = (client, site) else {
        report.push(Check::warn("stats", "not checked: no dashboard"));
        report.push(Check::warn("live meter", "not checked: no dashboard"));
        return;
    };
    let Ok(connection) = client.connection().await else {
        report.push(Check::warn(
            "stats",
            "not checked: no Home Assistant WebSocket",
        ));
        report.push(Check::warn(
            "live meter",
            "not checked: no Home Assistant WebSocket",
        ));
        return;
    };
    report.extend(stats::checks(&connection, site, now).await);
    report.push(live::check(client, site, now).await);
}

/// One real plan attempt; `None` when there is no connection or no dashboard to plan from.
async fn attempt(
    client: Option<&std::sync::Arc<HaClient>>,
    site: Option<&energy_ha::SiteModel>,
    options: &Options,
    devices: &[energy_battery::fleet::DeviceSnapshot],
    now: i64,
) -> Option<PlanProbe> {
    let (Some(client), Some(site)) = (client, site) else {
        return None;
    };
    if !site.configured {
        return None;
    }
    let connection = client.connection().await.ok()?;
    Some(plan_probe::run(connection, options, devices, now).await)
}

fn record(report: &mut Report, verbose: bool, phase: &str, mark: Instant) {
    if verbose {
        let millis = u64::try_from(mark.elapsed().as_millis()).unwrap_or(u64::MAX);
        report.timings.push((phase.to_owned(), millis));
    }
}

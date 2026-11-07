//! The Energy dashboard resolved from `energy/get_prefs` and the fields the planner needs.

use energy_ha::{HaClient, SiteModel, discover};

use crate::diag::check::Check;

/// Discover the dashboard and name the sources the plan requires.
pub async fn check(client: &HaClient) -> (Check, Option<SiteModel>) {
    let site = match client.connection().await {
        Ok(connection) => match discover(&connection).await {
            Ok(site) => site,
            Err(error) => {
                return (
                    Check::fail(
                        "energy dashboard",
                        format!("energy/get_prefs failed: {error}"),
                    ),
                    None,
                );
            }
        },
        Err(error) => {
            return (
                Check::fail("energy dashboard", format!("no WebSocket: {error}")),
                None,
            );
        }
    };
    if !site.configured {
        return (
            Check::fail(
                "energy dashboard",
                "no Energy dashboard configured (energy/get_prefs not_found)",
            ),
            Some(site),
        );
    }
    let detail = detail(&site);
    match required(&site) {
        Some(reason) => (
            Check::fail("energy dashboard", format!("{reason}; {detail}")),
            Some(site),
        ),
        None => match optional(&site) {
            Some(reason) => (
                Check::warn("energy dashboard", format!("{reason}; {detail}")),
                Some(site),
            ),
            None => (Check::pass("energy dashboard", detail), Some(site)),
        },
    }
}

/// Every configured field, including the power rules the live meter resolves through.
fn detail(site: &SiteModel) -> String {
    let grid_rate = rate(site.grid.power.as_ref());
    let solar_rate = rate(site.solar.power.as_ref());
    let forecast = site.solar.forecast_entry_ids.join(",");
    let batteries: Vec<String> = site
        .batteries
        .iter()
        .map(|battery| {
            format!(
                "soc {}, capacity {}",
                battery.soc.as_deref().unwrap_or("-"),
                battery
                    .capacity
                    .map_or_else(|| "-".to_owned(), |kwh| format!("{kwh} kWh"))
            )
        })
        .collect();
    format!(
        "grid {}/{}, grid rate {grid_rate}, solar {}, solar rate {solar_rate}, forecast {}, batteries {}, device_consumption {}, {}",
        site.grid.import.as_deref().unwrap_or("-"),
        site.grid.export.as_deref().unwrap_or("-"),
        site.solar.energy.as_deref().unwrap_or("-"),
        if forecast.is_empty() { "-" } else { &forecast },
        site.batteries.len(),
        site.devices.len(),
        if batteries.is_empty() {
            "-".to_owned()
        } else {
            batteries.join("; ")
        },
    )
}

fn rate(rule: Option<&energy_ha::PowerRule>) -> &'static str {
    if rule.is_some() { "ok" } else { "missing" }
}

/// The missing required source (a failure).
fn required(site: &SiteModel) -> Option<&'static str> {
    if site.grid.import.as_deref().is_none_or(str::is_empty) {
        return Some("no grid import statistic");
    }
    if site.grid.export.as_deref().is_none_or(str::is_empty) {
        return Some("no grid export statistic");
    }
    if site.solar.energy.as_deref().is_none_or(str::is_empty) {
        return Some("no solar production statistic");
    }
    None
}

/// A degraded optional source (a warning).
fn optional(site: &SiteModel) -> Option<&'static str> {
    if site.batteries.is_empty() {
        return Some("no battery source in the dashboard");
    }
    if site.solar.forecast_entry_ids.is_empty() {
        return Some("no solar forecast entry; the plan is degraded");
    }
    if site.grid.power.is_none() {
        return Some("no grid power (stat_rate/power_config); the live meter cannot resolve");
    }
    if site.batteries.iter().any(|battery| battery.soc.is_none()) {
        return Some("a battery source has no SoC statistic");
    }
    None
}

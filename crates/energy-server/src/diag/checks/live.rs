//! The live meter: grid, PV and load watts resolved from the dashboard's power entities, plus how
//! old the freshest reading is.

use std::collections::HashMap;

use energy_ha::{HaClient, PowerReading, SiteModel};

use crate::diag::check::Check;
use crate::diag::status::Status;

/// Read `/api/states` once and resolve the same live balance the engine's meter uses.
pub async fn check(client: &HaClient, site: &SiteModel, now: i64) -> Check {
    let states = match client.states().await {
        Ok(states) => states,
        Err(error) => return Check::fail("live meter", format!("GET /api/states failed: {error}")),
    };
    let mut readings = HashMap::new();
    for state in &states {
        if let Some(reading) = PowerReading::from_state(state) {
            readings.insert(state.entity_id.clone(), reading);
        }
    }
    let live = site.live(&readings);
    let age = age_s(&states, site, now);
    let detail = format!(
        "grid {}, PV {}, load {}, battery {}, soc {}, newest reading {}",
        watts(live.grid_w),
        watts(live.pv_w),
        watts(live.load_w),
        watts(live.battery_w),
        percent(live.soc_pct),
        age.map_or_else(|| "-".to_owned(), |age| format!("{age}s old")),
    );
    let status = status(&live, age);
    match status {
        Status::Fail => Check::fail("live meter", format!("no grid power reading; {detail}")),
        Status::Warn => Check::warn("live meter", detail),
        Status::Pass => Check::pass("live meter", detail),
    }
}

fn status(live: &energy_ha::Live, age: Option<i64>) -> Status {
    if live.grid_w.is_none() {
        return Status::Fail;
    }
    if live.pv_w.is_none() || live.load_w.is_none() || live.soc_pct.is_none() {
        return Status::Warn;
    }
    match age {
        Some(age) if age <= 300 => Status::Pass,
        _ => Status::Warn,
    }
}

/// Age of the stalest dashboard power/SoC entity, seconds.
fn age_s(states: &[energy_ha::EntityState], site: &SiteModel, now: i64) -> Option<i64> {
    let mut entities: Vec<&str> = Vec::new();
    for rule in [site.grid.power.as_ref(), site.solar.power.as_ref()]
        .into_iter()
        .flatten()
    {
        entities.extend(rule.entities());
    }
    for battery in &site.batteries {
        if let Some(rule) = &battery.power {
            entities.extend(rule.entities());
        }
        if let Some(soc) = &battery.soc {
            entities.push(soc);
        }
    }
    entities
        .iter()
        .filter_map(|id| {
            states
                .iter()
                .find(|state| state.entity_id == *id)
                .and_then(|state| state.last_updated)
        })
        .filter(|updated| updated.is_finite())
        .map(|updated| (now as f64 - updated).max(0.0) as i64)
        .max()
}

fn watts(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |watts| format!("{watts:.0} W"))
}

fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |percent| format!("{percent:.1}%"))
}

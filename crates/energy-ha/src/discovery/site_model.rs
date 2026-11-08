//! The normalized dashboard model consumed by the engine.

use std::collections::{BTreeMap, HashMap};

use super::{
    BatteryModel, DeviceModel, EnergyPreferences, EnergySource, GridModel, Live, PowerReading,
    PowerRule, SolarModel,
};

/// The normalized dashboard mapping.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SiteModel {
    /// False when `energy/get_prefs` reported `not_found` (no dashboard configured).
    pub configured: bool,
    pub grid: GridModel,
    pub solar: SolarModel,
    pub batteries: Vec<BatteryModel>,
    pub devices: Vec<DeviceModel>,
}

impl SiteModel {
    pub fn not_configured() -> Self {
        Self::default()
    }

    /// Build the normalized model. Unknown source types are skipped.
    pub fn from_prefs(prefs: &EnergyPreferences) -> Self {
        let mut site = Self {
            configured: true,
            ..Self::default()
        };
        let mut grid_rules = Vec::new();
        let mut solar_rules = Vec::new();
        for source in &prefs.energy_sources {
            site.apply_source(source, &mut grid_rules, &mut solar_rules);
        }
        site.grid.power = merge_power_rules(grid_rules);
        site.solar.power = merge_power_rules(solar_rules);
        site.devices = prefs
            .device_consumption
            .iter()
            .map(DeviceModel::from_prefs)
            .collect();
        site
    }

    fn apply_source(
        &mut self,
        source: &EnergySource,
        grid_rules: &mut Vec<PowerRule>,
        solar_rules: &mut Vec<PowerRule>,
    ) {
        match source {
            EnergySource::Grid(grid) => self.grid.merge(grid, grid_rules),
            EnergySource::Solar(solar) => self.solar.merge(solar, solar_rules),
            EnergySource::Battery(battery) => {
                self.batteries.push(BatteryModel::from_prefs(battery));
            }
            EnergySource::Unknown => {}
        }
    }

    /// Resolve the current snapshot from entity readings.
    ///
    /// Load follows `grid + pv + battery`, never the sum of devices (devices are a subset of load
    /// and can be included in another statistic). The battery term is signed positive while
    /// discharging, so a discharging pack adds to the house; the engine closes the balance from
    /// the fleet's own measurements when the dashboard has no battery power entity.
    pub fn live(&self, readings: &HashMap<String, PowerReading>) -> Live {
        let grid_w = self
            .grid
            .power
            .as_ref()
            .and_then(|rule| rule.resolve(readings));
        let pv_w = self
            .solar
            .power
            .as_ref()
            .and_then(|rule| rule.resolve(readings));
        let battery_w = resolve_batteries(&self.batteries, readings);
        let (load_w, load_clamped) = balance(grid_w, pv_w, battery_w);

        let mut devices_w = BTreeMap::new();
        for device in &self.devices {
            let Some(rule) = &device.power else {
                continue;
            };
            let Some(watts) = rule.resolve(readings) else {
                continue;
            };
            let key = device
                .name
                .clone()
                .or_else(|| device.consumption.clone())
                .unwrap_or_else(|| "device".to_owned());
            devices_w.insert(key, watts);
        }

        Live {
            grid_w,
            pv_w,
            battery_w,
            load_w,
            load_clamped,
            soc_pct: self.soc_pct(readings),
            devices_w,
        }
    }

    /// Capacity-weighted SoC over batteries with a finite 0–100 reading.
    ///
    /// A battery with `capacity > 0` is weighted by it. A battery without a usable capacity gets
    /// the mean of the configured capacities; if none are configured, all weights are 1.
    pub fn soc_pct(&self, readings: &HashMap<String, PowerReading>) -> Option<f64> {
        let socs: Vec<(Option<f64>, f64)> = self
            .batteries
            .iter()
            .filter_map(|battery| {
                let soc_id = battery.soc.as_ref()?;
                let soc = readings.get(soc_id)?.value;
                if !soc.is_finite() || !(0.0..=100.0).contains(&soc) {
                    return None;
                }
                Some((battery.capacity, soc))
            })
            .collect();
        if socs.is_empty() {
            return None;
        }

        let configured: Vec<f64> = self
            .batteries
            .iter()
            .filter_map(|battery| battery.capacity.filter(|capacity| *capacity > 0.0))
            .collect();
        let mean_configured = if configured.is_empty() {
            None
        } else {
            Some(configured.iter().sum::<f64>() / configured.len() as f64)
        };

        let mut weighted = 0.0;
        let mut weights = 0.0;
        for (capacity, soc) in socs {
            let weight = match capacity {
                Some(capacity) if capacity > 0.0 => capacity,
                _ => mean_configured.unwrap_or(1.0),
            };
            weighted += soc * weight;
            weights += weight;
        }
        if weights <= 0.0 {
            return None;
        }
        Some((weighted / weights).clamp(0.0, 100.0))
    }
}

fn merge_power_rules(mut rules: Vec<PowerRule>) -> Option<PowerRule> {
    match rules.len() {
        0 => None,
        1 => rules.pop(),
        _ => Some(PowerRule::Sum(rules)),
    }
}

fn resolve_batteries(
    batteries: &[BatteryModel],
    readings: &HashMap<String, PowerReading>,
) -> Option<f64> {
    if batteries.is_empty() {
        return None;
    }
    let mut total = 0.0;
    for battery in batteries {
        // Strict: a battery without a live-power source makes the total unknown, not partial.
        total += battery.power.as_ref()?.resolve(readings)?;
    }
    Some(total)
}

fn balance(grid: Option<f64>, pv: Option<f64>, battery: Option<f64>) -> (Option<f64>, bool) {
    match (grid, pv, battery) {
        (Some(grid), Some(pv), Some(battery)) => {
            let total = grid + pv + battery;
            if total < 0.0 {
                (Some(0.0), true)
            } else {
                (Some(total), false)
            }
        }
        _ => (None, false),
    }
}

//! One named load's meter as hourly kWh: an energy counter's `change`, or a power sensor's `mean`.
//!
//! The house balance is an energy counter, so the device split was written against `change`
//! statistics only. Real installs also expose a heat pump as a *power* sensor (no `change` at all),
//! so this module asks the recorder what the statistic is and converts either shape to the one
//! hourly kWh series the split needs. Anything else (a temperature, a percentage, a state) is
//! refused with a reason instead of being subtracted as garbage.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use energy_ha::{Connection, mean_series_hourly, slot_series_hourly, statistic_metadata_for};

/// How one hourly mean becomes kWh.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Meter {
    /// Already energy: the counter's `change`, converted to kWh by the recorder.
    Energy,
    /// A power statistic: `mean × 1 h`, scaled from the unit to kW.
    Power(f64),
}

/// The hourly kWh series for one device meter, or the reason it cannot be one.
pub async fn hourly_kwh(
    connection: &Connection,
    entity: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<BTreeMap<DateTime<Utc>, f64>, String> {
    let metadata = statistic_metadata_for(connection, entity)
        .await
        .map_err(|error| format!("its statistics metadata is unreadable: {error}"))?;
    let Some(metadata) = metadata else {
        return Err(
            "Home Assistant keeps no long-term statistics for it, so there is no hourly history to \
             split"
                .to_string(),
        );
    };
    let unit = metadata
        .statistics_unit_of_measurement
        .or(metadata.display_unit_of_measurement)
        .unwrap_or_default();
    let meter = meter_for(metadata.unit_class.as_deref(), &unit).ok_or_else(|| {
        format!(
            "its unit is {unit:?} (class {:?}), which is neither an energy counter nor a power \
             sensor",
            metadata.unit_class
        )
    })?;
    match meter {
        Meter::Energy => {
            let slots = slot_series_hourly(connection, entity, from, to)
                .await
                .map_err(|error| error.to_string())?;
            let mut series = BTreeMap::new();
            for slot in slots {
                if let (Some(kwh), Some(hour)) = (
                    slot.kwh,
                    DateTime::<Utc>::from_timestamp_millis(slot.start_ms),
                ) {
                    series.insert(hour, kwh);
                }
            }
            Ok(series)
        }
        Meter::Power(scale) => {
            let means = mean_series_hourly(connection, entity, from, to)
                .await
                .map_err(|error| error.to_string())?;
            Ok(means
                .into_iter()
                .map(|(hour, mean)| (hour, mean * scale))
                .collect())
        }
    }
}

/// The meter shape for one unit, or `None` when the statistic is neither energy nor power.
///
/// The class wins when the recorder reports one; otherwise the unit string decides. A power unit
/// is scaled to kW, which is what one hour of mean power is worth in kWh.
fn meter_for(class: Option<&str>, unit: &str) -> Option<Meter> {
    match class {
        Some("energy") => return Some(Meter::Energy),
        Some("power") => return power_scale(unit).map(Meter::Power),
        _ => {}
    }
    if power_scale(unit).is_some() {
        return power_scale(unit).map(Meter::Power);
    }
    matches!(unit.trim(), "Wh" | "kWh" | "MWh").then_some(Meter::Energy)
}

fn power_scale(unit: &str) -> Option<f64> {
    match unit.trim() {
        "W" => Some(0.001),
        "kW" => Some(1.0),
        "MW" => Some(1_000.0),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{Meter, meter_for};

    #[test]
    fn energy_counters_and_power_sensors_are_recognised_by_class_or_unit() {
        assert_eq!(meter_for(Some("energy"), "Wh"), Some(Meter::Energy));
        assert_eq!(meter_for(Some("power"), "W"), Some(Meter::Power(0.001)));
        assert_eq!(meter_for(None, "kWh"), Some(Meter::Energy));
        assert_eq!(meter_for(None, "kW"), Some(Meter::Power(1.0)));
    }

    #[test]
    fn a_temperature_sensor_is_not_a_meter() {
        assert_eq!(meter_for(Some("temperature"), "°C"), None);
        assert_eq!(meter_for(None, "%"), None);
    }
}

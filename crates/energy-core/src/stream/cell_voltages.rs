//! The cell-level readings the UI shows for one pack.

use crate::dispatch::PackReading;

/// A Li-ion cell cannot plausibly report more than this, V.
const MAX_CELL_V: f64 = 5.0;
/// A whole pack cannot plausibly report more than this, V.
const MAX_PACK_V: f64 = 100.0;
/// Temperature bounds wide enough for a freezing or sun-warmed pack, °C.
const MIN_TEMP_C: f64 = -50.0;
const MAX_TEMP_C: f64 = 100.0;
/// Cells in one pack stay far closer than this; a wider gap is a decode artefact, V.
const MAX_SPREAD_V: f64 = 1.0;

/// What the device published about the cells of one pack.
///
/// A pack may answer with an empty `packData` (it happens while full), so every number is optional.
/// `spread_mv` is only derived when both cell voltages are present and plausible; the guard keeps
/// using [`PackReading::min_cell_v`] unchanged and never sees this presentation value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CellVoltages {
    /// Lowest cell voltage, V.
    pub min_cell_v: Option<f64>,
    /// Highest cell voltage, V.
    pub max_cell_v: Option<f64>,
    /// `max_cell_v - min_cell_v`, mV, rounded to whole mV.
    pub spread_mv: Option<f64>,
    /// Total pack voltage, V.
    pub pack_voltage_v: Option<f64>,
    /// Highest pack temperature, °C.
    pub max_temp_c: Option<f64>,
}

impl CellVoltages {
    /// The display values behind one fleet reading, with bad numbers treated as absent.
    pub fn from_reading(reading: &PackReading) -> Self {
        let min_cell_v = within(reading.min_cell_v, 0.0, MAX_CELL_V);
        let max_cell_v = within(reading.max_cell_v, 0.0, MAX_CELL_V);
        let spread_mv = min_cell_v
            .zip(max_cell_v)
            .filter(|(low, high)| high >= low && high - low <= MAX_SPREAD_V)
            .map(|(low, high)| ((high - low) * 1000.0).round());
        Self {
            min_cell_v,
            max_cell_v,
            spread_mv,
            pack_voltage_v: within(reading.total_voltage_v, 0.0, MAX_PACK_V),
            max_temp_c: within(reading.max_temp_c, MIN_TEMP_C, MAX_TEMP_C),
        }
    }
}

/// A reading outside its physical range is not a measurement we should print.
fn within(value: Option<f64>, low: f64, high: f64) -> Option<f64> {
    value.filter(|number| number.is_finite() && *number > low && *number < high)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::PackReading;
    use crate::plan::PackSpec;

    fn reading(min: Option<f64>, max: Option<f64>) -> PackReading {
        PackReading {
            ledger_id: None,
            spec: PackSpec {
                pack_id: "SN-CELLS".to_owned(),
                capacity_kwh: 1.92,
                min_soc_pct: 5.0,
                max_soc_pct: 100.0,
                max_charge_w: 800.0,
                max_discharge_w: 800.0,
                eta_charge: 0.95,
                eta_discharge: 0.95,
            },
            soc_pct: Some(60.0),
            device_soc_pct: Some(60.0),
            soc_limit: energy_battery::SocLimit::None,
            fault_level: None,
            energy_kwh: Some(1.15),
            power_w: 0.0,
            flow: None,
            flow_alarm: None,
            active_fault: None,
            historical_fault: None,
            min_cell_v: min,
            max_cell_v: max,
            total_voltage_v: Some(49.7),
            max_temp_c: Some(29.1),
            observed_at: 0,
            available: true,
            stale: false,
        }
    }

    #[test]
    fn both_cell_voltages_give_a_spread_in_mv() {
        let cells = CellVoltages::from_reading(&reading(Some(3.32), Some(3.35)));
        assert_eq!(cells.min_cell_v, Some(3.32));
        assert_eq!(cells.max_cell_v, Some(3.35));
        assert_eq!(cells.spread_mv, Some(30.0));
        assert_eq!(cells.pack_voltage_v, Some(49.7));
        assert_eq!(cells.max_temp_c, Some(29.1));
    }

    #[test]
    fn one_missing_cell_leaves_the_spread_absent() {
        let cells = CellVoltages::from_reading(&reading(None, Some(3.35)));
        assert_eq!(cells.min_cell_v, None);
        assert_eq!(cells.max_cell_v, Some(3.35));
        assert_eq!(cells.spread_mv, None);

        let cells = CellVoltages::from_reading(&reading(Some(3.32), None));
        assert_eq!(cells.min_cell_v, Some(3.32));
        assert_eq!(cells.max_cell_v, None);
        assert_eq!(cells.spread_mv, None);
    }

    #[test]
    fn an_implausible_cell_is_treated_as_absent() {
        let cells = CellVoltages::from_reading(&reading(Some(0.0), Some(3.35)));
        assert_eq!(cells.min_cell_v, None);
        assert_eq!(cells.spread_mv, None);

        let cells = CellVoltages::from_reading(&reading(Some(3.32), Some(30.0)));
        assert_eq!(cells.max_cell_v, None);
        assert_eq!(cells.spread_mv, None);
    }

    #[test]
    fn an_inverted_pair_is_not_a_spread() {
        let cells = CellVoltages::from_reading(&reading(Some(3.35), Some(3.32)));
        assert_eq!(cells.spread_mv, None);
    }

    #[test]
    fn an_empty_report_has_no_cell_values_at_all() {
        let mut empty = reading(None, None);
        empty.total_voltage_v = None;
        empty.max_temp_c = None;
        assert_eq!(CellVoltages::from_reading(&empty), CellVoltages::default());
    }
}

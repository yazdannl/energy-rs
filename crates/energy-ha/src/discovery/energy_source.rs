//! One configured energy source; irrelevant types are skipped.

use serde::Deserialize;

use super::{BatteryPrefs, GridPrefs, SolarPrefs};

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EnergySource {
    Grid(GridPrefs),
    Solar(SolarPrefs),
    Battery(BatteryPrefs),
    #[serde(other)]
    Unknown,
}

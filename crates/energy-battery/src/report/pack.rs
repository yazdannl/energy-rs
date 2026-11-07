//! One pack's decoded state.

use serde_json::Value;

use crate::report::decode;
use crate::report::pack_label::PackLabel;

/// A pack as reported inside `packData`.
///
/// Cell voltages and the pack current are the protection inputs; the capacity label is a table
/// lookup, never a measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct PackReport {
    pub sn: String,
    pub pack_type: Option<i32>,
    pub soc_pct: f64,
    pub state: Option<i32>,
    pub power_w: i64,
    pub total_voltage_v: f64,
    pub max_cell_v: f64,
    pub min_cell_v: f64,
    pub max_temp_c: f64,
    pub current_a: f64,
    pub soft_version: Option<String>,
    pub heat_state: Option<i32>,
    pub label: Option<PackLabel>,
}

impl PackReport {
    /// Parse one `packData` entry; a pack without a serial is not a pack we can talk about.
    pub fn from_value(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let sn = decode::text(object, "sn")?;
        let pack_type = decode::int(object, "packType").map(|raw| raw as i32);
        let label = PackLabel::for_pack(&sn, pack_type);
        Some(Self {
            label,
            pack_type,
            soc_pct: decode::int(object, "socLevel").unwrap_or_default() as f64,
            state: decode::int(object, "state").map(|raw| raw as i32),
            power_w: decode::int(object, "power").unwrap_or_default(),
            total_voltage_v: decode::centivolts_to_volts(
                decode::int(object, "totalVol").unwrap_or_default(),
            ),
            max_cell_v: decode::centivolts_to_volts(
                decode::int(object, "maxVol").unwrap_or_default(),
            ),
            min_cell_v: decode::centivolts_to_volts(
                decode::int(object, "minVol").unwrap_or_default(),
            ),
            max_temp_c: decode::deci_kelvin_to_celsius(
                decode::int(object, "maxTemp").unwrap_or(2731),
            ),
            current_a: decode::signed_deci_amps(decode::int(object, "batcur").unwrap_or_default()),
            soft_version: decode::int(object, "softVersion").map(decode::packed_version),
            heat_state: decode::int(object, "heatState").map(|raw| raw as i32),
            sn,
        })
    }
}

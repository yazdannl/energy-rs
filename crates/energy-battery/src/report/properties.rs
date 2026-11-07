//! The decoded property set of one report.

use serde_json::{Map, Value};

use crate::report::decode;
use crate::report::grid_reverse::GridReverse;
use crate::report::soc_limit::SocLimit;

/// Every property this client reads, already in real units.
///
/// "Live" values (`inputLimit`, `chargeMaxLimit`, …) are what the device actually reports; the
/// controller clamps against them instead of a model table, because the tables run behind the
/// hardware.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Properties {
    pub electric_level_pct: f64,
    /// 1 = charge, 2 = discharge, 0 = not reported, anything else = wedged.
    pub ac_mode: i32,
    pub input_limit_w: u32,
    pub output_limit_w: u32,
    /// 1 = values kept in RAM, 0 = also written to flash.
    pub smart_mode: i32,
    pub soc_set_pct: f64,
    pub min_soc_pct: f64,
    pub soc_limit: SocLimit,
    /// `chargeMaxLimit`, 0 when the device does not report one.
    pub charge_max_w: u32,
    /// `inverseMaxPower`, 0 when the device does not report one.
    pub inverse_max_w: u32,
    pub grid_reverse: GridReverse,
    pub old_mode: Option<i32>,
    pub grid_input_w: i64,
    pub output_home_w: i64,
    pub solar_input_w: i64,
    pub output_pack_w: i64,
    pub pack_input_w: i64,
    /// Sum of the `solarPowerN` channels the firmware actually reports (2 on an 800 Plus).
    pub solar_power_w: i64,
    pub battery_voltage_v: f64,
    pub temperature_c: f64,
    pub pack_num: u32,
    pub is_error: bool,
    pub fault_level: Option<i32>,
    pub local_api_enable: Option<bool>,
    pub hems_state: Option<i32>,
    /// Minutes of runtime left; the 999-hour sentinel is `None`.
    pub remain_out_min: Option<u32>,
    pub write_rsp: Option<i32>,
    pub ts: Option<i64>,
}

/// Decode a merged property map. Missing keys keep their default; nothing is guessed from a model.
pub fn parse_properties(map: &Map<String, Value>) -> Properties {
    Properties {
        electric_level_pct: decode::int(map, "electricLevel").unwrap_or_default() as f64,
        ac_mode: decode::int(map, "acMode").unwrap_or_default() as i32,
        input_limit_w: decode::watts(decode::int(map, "inputLimit").unwrap_or_default()),
        output_limit_w: decode::watts(decode::int(map, "outputLimit").unwrap_or_default()),
        smart_mode: decode::int(map, "smartMode").unwrap_or_default() as i32,
        soc_set_pct: decode::deci_percent(decode::int(map, "socSet").unwrap_or_default()),
        min_soc_pct: decode::deci_percent(decode::int(map, "minSoc").unwrap_or_default()),
        soc_limit: SocLimit::from_raw(decode::int(map, "socLimit").unwrap_or_default() as i32),
        charge_max_w: decode::watts(decode::int(map, "chargeMaxLimit").unwrap_or_default()),
        inverse_max_w: decode::watts(decode::int(map, "inverseMaxPower").unwrap_or_default()),
        grid_reverse: GridReverse::from_raw(
            decode::int(map, "gridReverse").unwrap_or_default() as i32
        ),
        old_mode: decode::int_any(map, &["oldMode", "OldMode"]).map(|raw| raw as i32),
        grid_input_w: decode::int(map, "gridInputPower").unwrap_or_default(),
        output_home_w: decode::int(map, "outputHomePower").unwrap_or_default(),
        solar_input_w: decode::int(map, "solarInputPower").unwrap_or_default(),
        output_pack_w: decode::int(map, "outputPackPower").unwrap_or_default(),
        pack_input_w: decode::int(map, "packInputPower").unwrap_or_default(),
        solar_power_w: solar_channels(map),
        battery_voltage_v: decode::centivolts_to_volts(
            decode::int(map, "BatVolt").unwrap_or_default(),
        ),
        temperature_c: decode::deci_kelvin_to_celsius(decode::int(map, "hyperTmp").unwrap_or(2731)),
        pack_num: decode::watts(decode::int(map, "packNum").unwrap_or_default()),
        is_error: decode::int(map, "is_error").is_some_and(|raw| raw != 0),
        fault_level: decode::int(map, "faultLevel").map(|raw| raw as i32),
        local_api_enable: decode::flag(map, "localApiEnable"),
        hems_state: decode::int(map, "hemsState").map(|raw| raw as i32),
        remain_out_min: decode::int(map, "remainOutTime").and_then(decode::remain_minutes),
        write_rsp: decode::int(map, "writeRsp").map(|raw| raw as i32),
        ts: decode::int(map, "ts"),
    }
}

fn solar_channels(map: &Map<String, Value>) -> i64 {
    (1..=6)
        .filter_map(|index| decode::int(map, &format!("solarPower{index}")))
        .sum()
}

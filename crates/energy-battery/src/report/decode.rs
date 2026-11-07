//! Unit conversions and the tolerant raw readers, kept pure so fixtures can pin them down.
//!
//! Every rule here is tagged in `reports/energy-rs/zensdk.md` §3.1. The report publishes no energy
//! counter, so nothing in this module invents kWh from voltages.

use serde_json::{Map, Value};

/// Deci-percent: `socSet` 1000 means 100.0 %, `minSoc` 100 means 10.0 %.
pub fn deci_percent(raw: i64) -> f64 {
    raw as f64 / 10.0
}

/// Centivolts: pack `totalVol` 4810 means 48.10 V (the vendor doc's "volts" is wrong).
///
/// The same 1/100 scaling applies to `maxVol`/`minVol`/`BatVolt`, which are already called volts.
pub fn centivolts_to_volts(raw: i64) -> f64 {
    raw as f64 / 100.0
}

/// Pack current: signed 16-bit deci-amperes, `(raw >= 0x8000 ? raw - 0x10000 : raw) / 10` A.
pub fn signed_deci_amps(raw: i64) -> f64 {
    let signed = if raw >= 0x8000 { raw - 0x1_0000 } else { raw };
    signed as f64 / 10.0
}

/// Pack/inverter temperature in 0.1 K: `(raw - 2731) / 10` °C.
pub fn deci_kelvin_to_celsius(raw: i64) -> f64 {
    (raw as f64 - 2731.0) / 10.0
}

/// Watts as a non-negative magnitude; a negative or absurd raw value becomes 0 or the u32 ceiling.
pub fn watts(raw: i64) -> u32 {
    u32::try_from(raw.max(0)).unwrap_or(u32::MAX)
}

/// Packed firmware: `0x1103` (4355) is "1.1.3".
pub fn packed_version(raw: i64) -> String {
    let major = (raw >> 12) & 0xF;
    let minor = (raw >> 8) & 0xF;
    let patch = raw & 0xFF;
    format!("{major}.{minor}.{patch}")
}

/// Minutes, with 59940 (= 999 h) as the "invalid/infinite" sentinel.
pub fn remain_minutes(raw: i64) -> Option<u32> {
    if raw == 59940 || raw < 0 {
        None
    } else {
        u32::try_from(raw).ok()
    }
}

/// First present integer among `keys` (some firmware spells `oldMode` as `OldMode`).
pub(crate) fn int_any(map: &Map<String, Value>, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| int(map, key))
}

/// Tolerant integer reader: JSON numbers, integral floats and numeric strings all read.
pub(crate) fn int(map: &Map<String, Value>, key: &str) -> Option<i64> {
    let value = map.get(key)?;
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|raw| raw as i64))
        .or_else(|| value.as_str().and_then(|raw| raw.trim().parse().ok()))
}

/// Boolean reader for firmware that sends `true`/`false` or `1`/`0`.
pub(crate) fn flag(map: &Map<String, Value>, key: &str) -> Option<bool> {
    let value = map.get(key)?;
    value
        .as_bool()
        .or_else(|| value.as_i64().map(|raw| raw != 0))
}

/// String reader (model/product hints only, never machine decisions).
pub(crate) fn text(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

//! Unit conversion and rounding for DTO fields.
//!
//! Every DTO field name carries its unit (`powerW`, `energyWh`, `capacityKwh`, `buyEurPerKwh`), so
//! these helpers keep the arithmetic in one place. Rounded to milliwatts / milli-Wh so a plan slot
//! serializes as `1250.0` instead of `1249.9999999999998`.

/// Round `value` to `scale` decimals (10ⁿ).
fn round(value: f64, scale: f64) -> f64 {
    if value.is_finite() {
        (value * scale).round() / scale
    } else {
        0.0
    }
}

/// W with milli-watt resolution.
pub(crate) fn watts(value: f64) -> f64 {
    round(value, 1_000.0)
}

/// Wh with milli-Wh resolution.
pub(crate) fn watt_hours(value: f64) -> f64 {
    round(value, 1_000.0)
}

/// Hours with milli-hour (3.6 s) resolution.
pub(crate) fn hours(value: f64) -> f64 {
    round(value, 1_000.0)
}

/// kWh to Wh.
pub(crate) fn kwh_to_wh(value: f64) -> f64 {
    watt_hours(value * 1_000.0)
}

/// kW to W.
pub(crate) fn kw_to_w(value: f64) -> f64 {
    watts(value * 1_000.0)
}

/// Average kW over `slot_seconds` to Wh.
pub(crate) fn kw_slot_to_wh(value: f64, slot_seconds: i64) -> f64 {
    watt_hours(value * 1_000.0 * (slot_seconds as f64) / 3_600.0)
}

//! Solar reserve is reported headroom only; it is outside the override ladder.

use crate::safety::input::SafetyInput;

/// Result of the advisory solar-headroom calculation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolarHeadroom {
    /// PV-sourced charge allowance for the horizon, kWh. Never a charge ceiling.
    pub kwh: f64,
    /// Reason explaining the value, including missing telemetry.
    pub reason: &'static str,
    /// Forecast future PV, reported so a sunless night cannot be mistaken for surplus.
    pub forecast_future_pv_kwh: f64,
}

impl SolarHeadroom {
    /// Advisory headroom for one snapshot. Missing or stale telemetry grants nothing.
    pub fn evaluate(input: &SafetyInput) -> Self {
        let telemetry = input
            .solar_sample_at_ms
            .is_some_and(|at| input.at_ms - at <= TELEMETRY_FRESH_MS);
        if input.load_w.is_none() {
            return Self::missing(input, "solar_load_telemetry_missing");
        }
        let (Some(pv_w), Some(load_w)) = (input.pv_w, input.load_w) else {
            return Self::missing(input, "solar_telemetry_missing");
        };
        if !telemetry {
            return Self::missing(input, "solar_telemetry_missing");
        }
        if !pv_w.is_finite() || !load_w.is_finite() || input.horizon_hours <= 0.0 {
            return Self::missing(input, "solar_telemetry_missing");
        }
        let surplus_kwh = (pv_w - load_w).max(0.0) * input.horizon_hours / 1_000.0;
        let kwh = (surplus_kwh - input.solar_reserve_kwh).max(0.0);
        Self {
            kwh,
            reason: if kwh > 0.0 {
                "solar_headroom_available"
            } else {
                "no_measured_pv_surplus"
            },
            forecast_future_pv_kwh: input.forecast_future_pv_kwh,
        }
    }

    fn missing(input: &SafetyInput, reason: &'static str) -> Self {
        Self {
            kwh: 0.0,
            reason,
            forecast_future_pv_kwh: input.forecast_future_pv_kwh,
        }
    }
}

/// Freshness bound for solar/load telemetry used in the advisory headroom.
pub const TELEMETRY_FRESH_MS: i64 = 30_000;

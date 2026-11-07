//! The temperature regression stored beside the profile; only `b` and `r2` are ever applied.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::sample::LoadSample;
use crate::stats::{fsum, mean};
use crate::time::hour_floor;

/// The slope is only applied at serving time when `r2` reaches this.
pub const MIN_TEMPERATURE_R2: f64 = 0.05;

/// The fitted temperature term. `a` is stored for completeness but never applied: the learned
/// median already carries the level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TemperatureFit {
    /// Intercept, when the regression was fitted.
    pub a: Option<f64>,
    /// Slope, kW per degree C.
    pub b: f64,
    /// Coefficient of determination.
    pub r2: f64,
    /// Mean temperature of the fitted pairs, degrees C.
    pub t_mean: Option<f64>,
}

impl TemperatureFit {
    /// No fitted term.
    pub fn none() -> Self {
        Self {
            a: None,
            b: 0.0,
            r2: 0.0,
            t_mean: None,
        }
    }
}

/// Fit `load_kw = a + b * temperature` over the samples that have a temperature reading.
///
/// Requires at least 8 pairs and a positive temperature variance; below that the fit is empty
/// rather than fabricated.
pub fn fit_temperature(
    samples: &[LoadSample],
    temperatures: &BTreeMap<DateTime<Utc>, f64>,
) -> TemperatureFit {
    let mut pairs: Vec<(f64, f64)> = Vec::new();
    for sample in samples {
        if let Some(temperature) = temperatures.get(&hour_floor(sample.moment)) {
            pairs.push((*temperature, sample.load_kw));
        }
    }
    if pairs.len() < 8 {
        return TemperatureFit::none();
    }
    let temps: Vec<f64> = pairs.iter().map(|pair| pair.0).collect();
    let loads: Vec<f64> = pairs.iter().map(|pair| pair.1).collect();
    let temp_mean = mean(&temps);
    let load_mean = mean(&loads);
    let covariance = fsum(
        pairs
            .iter()
            .map(|(temperature, load)| (temperature - temp_mean) * (load - load_mean)),
    );
    let variance = fsum(
        temps
            .iter()
            .map(|value| (value - temp_mean) * (value - temp_mean)),
    );
    if variance <= 0.0 {
        return TemperatureFit {
            a: None,
            b: 0.0,
            r2: 0.0,
            t_mean: Some(temp_mean),
        };
    }
    let slope = covariance / variance;
    let intercept = load_mean - slope * temp_mean;
    let total = fsum(
        loads
            .iter()
            .map(|value| (value - load_mean) * (value - load_mean)),
    );
    let residual = fsum(pairs.iter().map(|(temperature, load)| {
        let predicted = intercept + slope * temperature;
        (load - predicted) * (load - predicted)
    }));
    let r2 = if total <= 0.0 {
        0.0
    } else {
        (1.0 - residual / total).max(0.0)
    };
    TemperatureFit {
        a: Some(intercept),
        b: slope,
        r2,
        t_mean: Some(temp_mean),
    }
}

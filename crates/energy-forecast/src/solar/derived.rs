//! Fitting the array size implied by meter-export samples (the derived source).

use chrono::{DateTime, Timelike, Utc};
use chrono_tz::Tz;

use crate::physics::{CapacityConfig, Site};

use super::capacity::capacity_poa;

/// Minimum usable daylight samples before the fit is trusted.
pub const DERIVED_MIN_SAMPLES: usize = 24;
/// The ratio envelope quantile that becomes the array size.
pub const DERIVED_ENVELOPE_QUANTILE: f64 = 0.9;
/// Ratios with a smaller model output per installed watt are skipped as noise.
pub const DERIVED_MIN_MODEL_KW_PER_WP: f64 = 1e-5;

/// The fitted array size and how many samples stood behind it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImpliedCapacity {
    /// Fitted DC capacity, Wp; 0.0 when there is not enough daylight.
    pub capacity_wp: f64,
    /// Usable samples.
    pub samples: usize,
}

/// Array size (Wp) implied by meter-derived PV samples: the upper envelope of `pv / model`.
///
/// Every sample is divided by the clear-sky model's output **per installed watt** at that moment,
/// and the 90th percentile of those ratios is the array size: overcast hours pull a ratio down, so
/// the top of the distribution is the cloudless estimate. `clearness` (NASA's per-local-hour
/// observed ratio) removes some of that bias first.
pub fn implied_capacity_wp(
    site: &Site,
    samples: &[(DateTime<Utc>, f64)],
    config: &CapacityConfig,
    clearness: Option<&[f64; 24]>,
    zone: Tz,
) -> ImpliedCapacity {
    let ratios = usable_ratios(site, samples, config, clearness, zone);
    if ratios.len() < DERIVED_MIN_SAMPLES {
        return ImpliedCapacity {
            capacity_wp: 0.0,
            samples: ratios.len(),
        };
    }
    let index = envelope_index(ratios.len());
    ImpliedCapacity {
        capacity_wp: ratios[index].max(0.0),
        samples: ratios.len(),
    }
}

/// `pv_kw / model_kw_per_wp` for every sample with a usable model output, sorted ascending.
fn usable_ratios(
    site: &Site,
    samples: &[(DateTime<Utc>, f64)],
    config: &CapacityConfig,
    clearness: Option<&[f64; 24]>,
    zone: Tz,
) -> Vec<f64> {
    let mut ratios: Vec<f64> = Vec::with_capacity(samples.len());
    for (stamp, pv_kw) in samples {
        let poa = capacity_poa(site, *stamp, zone);
        let hour = stamp.with_timezone(&zone).hour() as usize % 24;
        let observed = clearness.map_or(1.0, |profile| profile[hour]);
        let per_wp_kw = (poa / 1000.0) * observed * config.performance_ratio / 1000.0;
        if per_wp_kw <= DERIVED_MIN_MODEL_KW_PER_WP {
            continue;
        }
        ratios.push(*pv_kw / per_wp_kw);
    }
    ratios.sort_by(f64::total_cmp);
    ratios
}

/// Python's `min(n - 1, round(0.9 * (n - 1)))`, with half-even rounding.
#[expect(
    clippy::cast_sign_loss,
    reason = "the position is non-negative by construction: the quantile is in (0, 1) and n >= 1"
)]
fn envelope_index(samples: usize) -> usize {
    let last = samples - 1;
    let position = (DERIVED_ENVELOPE_QUANTILE * last as f64).round_ties_even();
    (position as usize).min(last)
}

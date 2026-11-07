//! The promotion gate: a candidate is adopted only by beating the incumbent on the holdout.

use super::head_fit::fit_factors;
use super::head_sample::HeadSample;
use super::head_status::HeadStatus;

/// Evidence required before a fit is even a candidate.
pub const MIN_SAMPLES: usize = 36;
/// Distinct local hours required before a fit is a candidate.
pub const MIN_HOURS: usize = 10;
/// The candidate must beat the incumbent by this fraction of its error.
pub const PROMOTION_MARGIN: f64 = 0.02;

/// What the last refit decided, for the status payload and the tests.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadReport {
    /// The gate's decision.
    pub status: HeadStatus,
    /// Whether the candidate replaces the incumbent.
    pub promoted: bool,
    /// Scored slots offered in total.
    pub samples: usize,
    /// Scored slots used to fit.
    pub fit_samples: usize,
    /// Scored slots used to gate.
    pub holdout_samples: usize,
    /// Candidate holdout MAE, kW.
    pub candidate_mae_kw: Option<f64>,
    /// Incumbent holdout MAE, kW.
    pub incumbent_mae_kw: Option<f64>,
    /// Human-readable reason.
    pub reason: String,
}

/// MAE of `forecast × factor[local hour]` against the measurement, kW.
pub fn mean_absolute_error(samples: &[HeadSample], factors: &[f64; 24]) -> Option<f64> {
    let mut total = 0.0;
    let mut count = 0usize;
    for sample in samples {
        if sample.forecast_kw <= 1e-6 {
            continue;
        }
        let factor = factors[(sample.hour % 24) as usize];
        total += (sample.forecast_kw * factor - sample.actual_kw).abs();
        count += 1;
    }
    if count == 0 {
        None
    } else {
        Some(total / count as f64)
    }
}

/// Fit and gate a candidate against the incumbent, without touching any store.
///
/// `fit` is the older slice of the window; `holdout` is the newest, because that is the regime the
/// head is about to be used in. An incumbent with no error cannot be improved on.
pub fn evaluate(fit: &[HeadSample], holdout: &[HeadSample], incumbent: &[f64; 24]) -> HeadReport {
    let samples = fit.len() + holdout.len();
    let hours: std::collections::BTreeSet<u32> =
        fit.iter().map(|sample| sample.hour % 24).collect();
    if fit.len() < MIN_SAMPLES || hours.len() < MIN_HOURS {
        return HeadReport {
            status: HeadStatus::Insufficient,
            promoted: false,
            samples,
            fit_samples: fit.len(),
            holdout_samples: holdout.len(),
            candidate_mae_kw: None,
            incumbent_mae_kw: None,
            reason: format!("need {MIN_SAMPLES} scored slots across {MIN_HOURS} hours of the day"),
        };
    }
    let candidate = fit_factors(fit, super::head_fit::SHRINKAGE);
    let candidate_mae = mean_absolute_error(holdout, &candidate);
    let incumbent_mae = mean_absolute_error(holdout, incumbent);
    let (status, reason) = match (candidate_mae, incumbent_mae) {
        (None, _) | (_, None) => (
            HeadStatus::Rejected,
            "nothing scored in the holdout window".to_string(),
        ),
        (Some(_), Some(incumbent_value)) if incumbent_value <= 0.0 => (
            HeadStatus::Rejected,
            "the incumbent is already exact over the holdout".to_string(),
        ),
        (Some(candidate_value), Some(incumbent_value))
            if candidate_value < incumbent_value * (1.0 - PROMOTION_MARGIN) =>
        {
            (
                HeadStatus::Promoted,
                format!(
                    "holdout MAE {candidate_value:.3} kW beats {incumbent_value:.3} kW by more than {:.0}%",
                    PROMOTION_MARGIN * 100.0
                ),
            )
        }
        (Some(candidate_value), Some(incumbent_value)) => (
            HeadStatus::Rejected,
            format!(
                "holdout MAE {candidate_value:.3} kW does not beat {incumbent_value:.3} kW by {:.0}%; keeping the incumbent",
                PROMOTION_MARGIN * 100.0
            ),
        ),
    };
    HeadReport {
        status,
        promoted: status == HeadStatus::Promoted,
        samples,
        fit_samples: fit.len(),
        holdout_samples: holdout.len(),
        candidate_mae_kw: candidate_mae,
        incumbent_mae_kw: incumbent_mae,
        reason,
    }
}

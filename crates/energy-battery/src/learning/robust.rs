//! Robust combination helpers: median, median-absolute-deviation trim and weighted median.

/// 1.4826 turns a median absolute deviation into a sigma estimate for normally distributed data.
pub(crate) const MAD_SCALE: f64 = 1.4826;

/// Median of the finite values; `None` when nothing finite is left.
///
/// An even count averages the two middle values, which is the plain definition and keeps a synthetic
/// half-and-half population from snapping to one side.
pub(crate) fn median(values: &[f64]) -> Option<f64> {
    let mut finite: Vec<f64> = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if finite.is_empty() {
        return None;
    }
    finite.sort_by(f64::total_cmp);
    let middle = finite.len() / 2;
    if finite.len() % 2 == 1 {
        Some(finite[middle])
    } else {
        Some(f64::midpoint(finite[middle - 1], finite[middle]))
    }
}

/// Which values a median-absolute-deviation filter keeps, aligned with `values`.
///
/// A zero MAD (more than half the values agree, or there is a single value) keeps every finite
/// value: the filter may never empty the set, and a pack whose sessions all agree exactly must not
/// be trimmed. A non-finite value is never kept.
pub(crate) fn mad_inliers(values: &[f64], limit: f64) -> Vec<bool> {
    let Some(middle) = median(values) else {
        return vec![false; values.len()];
    };
    let deviations: Vec<f64> = values.iter().map(|value| (value - middle).abs()).collect();
    let Some(mad) = median(&deviations) else {
        return vec![false; values.len()];
    };
    if mad <= 0.0 {
        return values.iter().map(|value| value.is_finite()).collect();
    }
    let cutoff = limit * MAD_SCALE * mad;
    let keep: Vec<bool> = values
        .iter()
        .map(|value| value.is_finite() && (value - middle).abs() <= cutoff)
        .collect();
    if keep.iter().any(|kept| *kept) {
        keep
    } else {
        values.iter().map(|value| value.is_finite()).collect()
    }
}

/// Weighted median of `(value, weight)` pairs; `None` when no pair has a positive weight.
///
/// Values are sorted and the first cumulative weight that reaches half the total is the median, so
/// one heavy sample pins the estimate while several light ones must agree to move it.
pub(crate) fn weighted_median(pairs: &[(f64, f64)]) -> Option<f64> {
    let mut usable: Vec<(f64, f64)> = pairs
        .iter()
        .copied()
        .filter(|(value, weight)| value.is_finite() && weight.is_finite() && *weight > 0.0)
        .collect();
    if usable.is_empty() {
        return None;
    }
    usable.sort_by(|left, right| left.0.total_cmp(&right.0));
    let total: f64 = usable.iter().map(|(_, weight)| weight).sum();
    let halfway = total / 2.0;
    let mut cumulative = 0.0;
    for (value, weight) in &usable {
        cumulative += weight;
        if cumulative >= halfway {
            return Some(*value);
        }
    }
    usable.last().map(|(value, _)| *value)
}

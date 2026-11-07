//! Python's `_quantile`: linear interpolation at `q * (n - 1)`.

/// Quantile of `values` at `fraction`, with the linear index `q * (n - 1)`.
///
/// This is the same formula NumPy's `method="linear"` uses, written explicitly: `load.py` does not
/// call NumPy and its own helper is the contract.
pub fn quantile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut ordered = values.to_vec();
    ordered.sort_by(f64::total_cmp);
    if ordered.len() == 1 {
        return ordered[0];
    }
    let position = fraction * (ordered.len() - 1) as f64;
    let lower = index_of(position);
    let upper = (lower + 1).min(ordered.len() - 1);
    let weight = position - lower as f64;
    ordered[lower] * (1.0 - weight) + ordered[upper] * weight
}

/// `position.floor()` as an index. `fraction` is in `(0, 1)` and the position is bounded by the
/// slice length, so the value is non-negative and inside the slice.
#[expect(
    clippy::cast_sign_loss,
    reason = "the quantile position is non-negative by construction"
)]
fn index_of(position: f64) -> usize {
    position.floor() as usize
}

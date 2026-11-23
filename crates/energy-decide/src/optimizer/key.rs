//! The canonical six-key tie-break, computed on the final executed path only.

/// One sentinel for "no discharge" in every canonical key, including suffixes and terminal nodes.
pub(crate) const NO_DISCHARGE: usize = usize::MAX;

pub(crate) type CanonicalKey = (i64, usize, i64, usize, Vec<i32>, Vec<i32>);

/// Objective at 1e-6 EUR, first discharge slot, exact sum of power squared in W², nonzero count,
/// 1 Wh energy signature and 1 W power signature. Slot duration never scales the p² key.
pub(crate) fn path_key(
    total_eur: f64,
    first_discharge: usize,
    power_squared_w2: i64,
    nonzero_slots: usize,
    energy_signature_wh: &[i32],
    power_signature_w: &[i32],
) -> CanonicalKey {
    (
        (total_eur * 1e6).round() as i64,
        first_discharge,
        power_squared_w2,
        nonzero_slots,
        energy_signature_wh.to_vec(),
        power_signature_w.to_vec(),
    )
}

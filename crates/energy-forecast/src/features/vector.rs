//! A fixed-size feature row: 28 `f64` in contract order.

use super::names::FEATURE_COUNT;
use crate::ForecastError;

/// One feature vector. A newtype over `[f64; 28]` so it cannot be built from a shorter slice and the
/// walk cannot be handed the wrong column count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Features([f64; FEATURE_COUNT]);

impl Features {
    /// Number of features, as a constant usable in array types.
    pub const COUNT: usize = FEATURE_COUNT;

    /// Wrap an array already in contract order.
    pub fn new(values: [f64; FEATURE_COUNT]) -> Self {
        Self(values)
    }

    /// Wrap a slice, refusing the wrong length.
    pub fn from_slice(values: &[f64]) -> Result<Self, ForecastError> {
        let array: [f64; FEATURE_COUNT] = values.try_into().map_err(|_| {
            ForecastError::FeatureContract(format!(
                "feature vector has {} values, the contract has {FEATURE_COUNT}",
                values.len()
            ))
        })?;
        Ok(Self(array))
    }

    /// The values in contract order.
    pub fn as_array(&self) -> &[f64; FEATURE_COUNT] {
        &self.0
    }

    /// One value by frozen index. An index outside the contract cannot come from a validated tree;
    /// it yields NaN, which the walk routes with `missing_left`, instead of panicking.
    pub fn get(&self, index: usize) -> f64 {
        self.0.get(index).copied().unwrap_or(f64::NAN)
    }

    /// The values as a vector, for serialisation in tests and benches.
    pub fn to_vec(&self) -> Vec<f64> {
        self.0.to_vec()
    }
}

//! One tree of the ensemble, still in bit patterns.

use serde::{Deserialize, Serialize};

use crate::ForecastError;

/// One tree of the ensemble: parallel arrays, bit-pattern thresholds and leaf values.
///
/// The bit patterns are `u64` because negative doubles have the sign bit set, so 14,219 of the
/// committed artifact's 46,673 patterns exceed `i64::MAX`. `is_leaf` and `missing_left` are 0/1
/// integers, not JSON booleans.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TreeArtifact {
    /// Feature index for internal nodes; 0 for leaves.
    pub feature: Vec<u16>,
    /// Binary64 split thresholds, as bit patterns; 0 for leaves.
    pub threshold_bits: Vec<u64>,
    /// Left child index; 0 for leaves.
    pub left: Vec<u32>,
    /// Right child index; 0 for leaves.
    pub right: Vec<u32>,
    /// Binary64 leaf values, as bit patterns; 0 for internal nodes.
    pub value_bits: Vec<u64>,
    /// The only valid leaf test: 1 = leaf.
    pub is_leaf: Vec<u8>,
    /// NaN routing: 1 = left, 0 = right.
    pub missing_left: Vec<u8>,
}

impl TreeArtifact {
    /// Node count, or an error when the parallel arrays disagree.
    pub fn node_count(&self) -> Result<usize, ForecastError> {
        let length = self.feature.len();
        let consistent = self.threshold_bits.len() == length
            && self.left.len() == length
            && self.right.len() == length
            && self.value_bits.len() == length
            && self.is_leaf.len() == length
            && self.missing_left.len() == length;
        if consistent {
            Ok(length)
        } else {
            Err(ForecastError::BadArtifact(format!(
                "tree arrays disagree: {length} features, {} thresholds, {} values",
                self.threshold_bits.len(),
                self.value_bits.len()
            )))
        }
    }

    /// Whether the tree has no nodes.
    pub fn is_empty(&self) -> bool {
        self.feature.is_empty()
    }
}

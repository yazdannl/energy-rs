//! The walk itself: one path from node zero to a leaf, exactly as `pv.py` walks it.

use super::tree::Tree;
use crate::ForecastError;
use crate::features::Features;

/// Walk one tree and return the reached leaf's value.
///
/// The comparison is `<=`, not `<`: rows sitting exactly on a threshold take the left branch. NaN
/// is the only special value, and `missing_left` decides its direction per node. Leaf detection is
/// `is_leaf` only. The step cap makes a corrupt cyclic artifact an error instead of a hang.
pub fn walk(tree: &Tree, features: &Features) -> Result<f64, ForecastError> {
    let mut node = 0usize;
    for _ in 0..=tree.len() {
        let current = tree.node(node)?;
        if current.is_leaf {
            return Ok(current.value);
        }
        let observed = features.get(usize::from(current.feature));
        node = if observed.is_nan() {
            if current.missing_left {
                current.left as usize
            } else {
                current.right as usize
            }
        } else if observed <= current.threshold {
            current.left as usize
        } else {
            current.right as usize
        };
    }
    Err(ForecastError::BadArtifact(
        "tree walk did not terminate: the tree has a cycle".to_string(),
    ))
}

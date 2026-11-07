//! The ensemble sum and the residual clamp around it.

use super::tree::Tree;
use super::walk::walk;
use crate::ForecastError;
use crate::features::Features;
use crate::model::{Artifact, RESIDUAL_MAX, RESIDUAL_MIN};

/// All trees plus the baseline and the clamp the artifact declares.
///
/// The clamp is applied to the **sum**, never per tree, and the leaf values already carry the
/// training pipeline's shrinkage: there is no learning rate here.
#[derive(Debug, Clone, PartialEq)]
pub struct Ensemble {
    trees: Vec<Tree>,
    baseline: f64,
    clamp: (f64, f64),
}

impl Ensemble {
    /// Decode every tree and validate the artifact's baseline and clamp.
    pub fn from_artifact(artifact: &Artifact) -> Result<Self, ForecastError> {
        let clamp = clamp_bounds(artifact.residual_clamp)?;
        let baseline = f64::from_bits(artifact.baseline_bits);
        if !baseline.is_finite() {
            return Err(ForecastError::BadArtifact(
                "baseline bit pattern is not a finite double".to_string(),
            ));
        }
        if artifact.trees.is_empty() {
            return Err(ForecastError::BadArtifact(
                "ensemble has no trees".to_string(),
            ));
        }
        let mut trees = Vec::with_capacity(artifact.trees.len());
        for (index, raw) in artifact.trees.iter().enumerate() {
            trees.push(Tree::from_artifact(raw, index)?);
        }
        Ok(Self {
            trees,
            baseline,
            clamp,
        })
    }

    /// The raw ensemble output: baseline plus the reached leaves, in artifact order.
    pub fn raw_ratio(&self, features: &Features) -> Result<f64, ForecastError> {
        let mut total = self.baseline;
        for tree in &self.trees {
            total += walk(tree, features)?;
        }
        Ok(total)
    }

    /// The served ratio: the raw sum clamped to the artifact's residual range.
    pub fn ratio(&self, features: &Features) -> Result<f64, ForecastError> {
        let raw = self.raw_ratio(features)?;
        Ok(clamp(raw, self.clamp))
    }

    /// The residual clamp this ensemble enforces.
    pub fn clamp(&self) -> (f64, f64) {
        self.clamp
    }

    /// Number of trees.
    pub fn tree_count(&self) -> usize {
        self.trees.len()
    }

    /// Number of nodes across all trees.
    pub fn node_count(&self) -> usize {
        self.trees.iter().map(Tree::len).sum()
    }

    /// Number of leaves across all trees.
    pub fn leaf_count(&self) -> usize {
        self.trees.iter().map(Tree::leaf_count).sum()
    }
}

/// Clamp `value` into `bounds`, returning the bound hit. NaN falls through unchanged, as it does
/// in Python's `max(min_bound, min(max_bound, value))` for a NaN value only when compared the
/// other way round; a NaN sum is refused upstream instead of being rounded to a bound.
fn clamp(value: f64, bounds: (f64, f64)) -> f64 {
    if value < bounds.0 {
        bounds.0
    } else if value > bounds.1 {
        bounds.1
    } else {
        value
    }
}

/// The artifact clamp, or the committed constants when absent. A malformed clamp is refused rather
/// than silently replaced: the range is part of the model's contract.
fn clamp_bounds(raw: Option<[f64; 2]>) -> Result<(f64, f64), ForecastError> {
    let (low, high) = match raw {
        Some([low, high]) => (low, high),
        None => (RESIDUAL_MIN, RESIDUAL_MAX),
    };
    if !low.is_finite() || !high.is_finite() || low >= high {
        return Err(ForecastError::BadArtifact(format!(
            "residual clamp [{low}, {high}] is not a usable range"
        )));
    }
    Ok((low, high))
}

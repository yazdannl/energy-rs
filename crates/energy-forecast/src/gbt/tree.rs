//! A tree decoded once at load: f64 thresholds and values, validated child pointers.

use super::node::Node;
use crate::ForecastError;
use crate::features::FEATURE_COUNT;
use crate::model::TreeArtifact;

/// One tree with its bit patterns already decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    /// Decode and validate one artifact tree. Every guard here is a real corruption check, not
    /// decoration: a dangling child index or a leaf flag outside 0/1 would otherwise surface as a
    /// panic in the walk or, worse, as a silently wrong forecast.
    pub fn from_artifact(raw: &TreeArtifact, index: usize) -> Result<Self, ForecastError> {
        let length = raw.node_count()?;
        if length <= 1 {
            return Err(ForecastError::BadArtifact(format!(
                "tree {index} has {length} node(s); at least two are required"
            )));
        }
        let mut nodes = Vec::with_capacity(length);
        for offset in 0..length {
            let is_leaf = match raw.is_leaf[offset] {
                0 => false,
                1 => true,
                other => {
                    return Err(ForecastError::BadArtifact(format!(
                        "tree {index} node {offset} has is_leaf={other}, expected 0 or 1"
                    )));
                }
            };
            let missing_left = match raw.missing_left[offset] {
                0 => false,
                1 => true,
                other => {
                    return Err(ForecastError::BadArtifact(format!(
                        "tree {index} node {offset} has missing_left={other}, expected 0 or 1"
                    )));
                }
            };
            let left = raw.left[offset];
            let right = raw.right[offset];
            if !is_leaf {
                if usize::from(raw.feature[offset]) >= FEATURE_COUNT {
                    return Err(ForecastError::BadArtifact(format!(
                        "tree {index} node {offset} uses feature {}, contract has {FEATURE_COUNT}",
                        raw.feature[offset]
                    )));
                }
                if left as usize >= length || right as usize >= length {
                    return Err(ForecastError::BadArtifact(format!(
                        "tree {index} node {offset} points outside the tree ({left}, {right} of {length})"
                    )));
                }
                if left == right {
                    return Err(ForecastError::BadArtifact(format!(
                        "tree {index} node {offset} has identical children ({left})"
                    )));
                }
            }
            nodes.push(Node {
                feature: raw.feature[offset],
                threshold: f64::from_bits(raw.threshold_bits[offset]),
                left,
                right,
                value: f64::from_bits(raw.value_bits[offset]),
                is_leaf,
                missing_left,
            });
        }
        Ok(Self { nodes })
    }

    /// The decoded nodes, in artifact order.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// One node, or an error when the index is outside the tree.
    pub fn node(&self, index: usize) -> Result<&Node, ForecastError> {
        self.nodes.get(index).ok_or_else(|| {
            ForecastError::BadArtifact(format!(
                "node {index} is outside a {}-node tree",
                self.nodes.len()
            ))
        })
    }

    /// Number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree has no nodes (never true for a validated tree).
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Number of leaves in this tree.
    pub fn leaf_count(&self) -> usize {
        self.nodes.iter().filter(|node| node.is_leaf).count()
    }
}

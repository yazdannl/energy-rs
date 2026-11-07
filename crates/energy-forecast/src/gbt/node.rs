//! One decoded node: f64 threshold and leaf value, no bit patterns left.

/// A tree node. Internal nodes carry a feature index, a threshold and two children; leaves carry a
/// value. `is_leaf` is stored explicitly because in this encoding a leaf's child pointers refer
/// back to node zero, so inferring a leaf from them misclassifies node zero's children.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    /// Feature index, 0..28, for internal nodes.
    pub feature: u16,
    /// Split threshold for internal nodes; meaningless for leaves.
    pub threshold: f64,
    /// Left child index; meaningless for leaves.
    pub left: u32,
    /// Right child index; meaningless for leaves.
    pub right: u32,
    /// Leaf value for leaves; meaningless for internal nodes.
    pub value: f64,
    /// The only valid leaf test.
    pub is_leaf: bool,
    /// Where a NaN feature goes: `true` = left, `false` = right.
    pub missing_left: bool,
}

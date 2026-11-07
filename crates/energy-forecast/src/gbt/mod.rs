//! The exact tree walk: `<` vs `<=`, per-node NaN routing, `is_leaf` as the only leaf test.

mod ensemble;
mod node;
mod tree;
mod walk;

pub use ensemble::Ensemble;
pub use node::Node;
pub use tree::Tree;
pub use walk::walk;

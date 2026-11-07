//! The `pv_base.json` artifact: schema, contract, decode and load.

mod artifact;
mod contract;
mod loader;
mod metadata;
mod pv_model;
mod tree_artifact;

pub use artifact::Artifact;
pub use contract::{
    LEARNED_KIND, RESIDUAL_MAX, RESIDUAL_MIN, feature_digest, validate_digest, validate_names,
};
pub use loader::{load_path, parse};
pub use metadata::ModelMetadata;
pub use pv_model::PvModel;
pub use tree_artifact::TreeArtifact;

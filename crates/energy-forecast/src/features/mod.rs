//! The 28 features in frozen order, and the two physics chains that feed them.

mod builder;
mod built;
mod names;
mod night;
mod vector;

pub use builder::{FeatureInputs, build};
pub use built::BuiltFeatures;
pub use names::{FEATURE_COUNT, FEATURE_NAMES, index};
pub use night::{MIN_ELEVATION_DEG, is_night};
pub use vector::Features;

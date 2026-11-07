//! One slot's learned PV estimate: geometry, features, walk and composition.

mod estimate;
mod slot_estimate;
mod slot_inputs;

pub use estimate::{estimate_slot, slot_features};
pub use slot_estimate::SlotEstimate;
pub use slot_inputs::SlotInputs;

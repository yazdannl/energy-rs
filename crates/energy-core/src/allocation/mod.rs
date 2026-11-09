//! Split one fleet setpoint across the packs, respecting each pack's own limits.

mod fleet_allocation;
mod pack_allocation;
mod split_input;

pub use fleet_allocation::Allocation;
pub use pack_allocation::PackAllocation;
pub use split_input::SplitInput;

/// Fleet demand below this many watts is a deadband, not a setpoint.
pub const ALLOCATION_DEADBAND_W: f64 = 30.0;

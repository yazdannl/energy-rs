//! Split one fleet setpoint across the packs, respecting each pack's own limits.

mod fleet_allocation;
mod pack_allocation;

pub use fleet_allocation::Allocation;
pub use pack_allocation::PackAllocation;

/// Fleet demand below this many watts is a deadband, not a setpoint.
pub const ALLOCATION_DEADBAND_W: f64 = 30.0;

//! Split one fleet setpoint across the packs, respecting each pack's own limits.
//! A pack whose device report closes charge is zeroed before the split and its share is
//! redistributed to the packs that can still accept it.

mod charge_skip;
mod fleet_allocation;
mod hold;
mod pack_allocation;
mod split_input;

pub use charge_skip::ChargeSkip;
pub use fleet_allocation::Allocation;
pub use hold::Hold;
pub use pack_allocation::PackAllocation;
pub use split_input::SplitInput;

/// Fleet demand below this many watts is a deadband, not a setpoint.
pub const ALLOCATION_DEADBAND_W: f64 = 30.0;

/// A pack that reported a charge ceiling stays out of the charge split until its device-reported
/// SoC falls at least this far below its ceiling. At a 100 % ceiling, 99 % is still locked out;
/// 98 % releases it. The margin stops a pack hovering on the limit from being re-commanded.
pub const CHARGE_RELEASE_HYSTERESIS_PCT: f64 = 2.0;

/// `faultLevel` 2 this close to the ceiling is the device's normal full-charge protection, not a
/// fault to back off or escalate. Below the band it stays an ordinary fault.
pub const FULL_CHARGE_FAULT_BAND_PCT: f64 = 2.0;

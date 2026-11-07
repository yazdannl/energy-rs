//! Protection: latching cell-voltage guard and full-charge maintenance.
//!
//! The guard blocks discharge below the cutoff, forces an emergency charge to restore voltage,
//! survives restarts and stays latched when readings go stale. Maintenance does a weekly full
//! charge for cell balancing.
//!
//! No code yet: P0 scaffold only.

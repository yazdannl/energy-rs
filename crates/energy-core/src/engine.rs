//! Engine loop.
//!
//! Wakes on slot boundaries, assembles inputs, asks `energy-opt` for a plan, stores it, and hands
//! the current slot's intent to the dispatcher. A missed or stale input holds the previous safe
//! setpoint; a failed validation refuses the plan. The loop never blocks on I/O for long enough to
//! miss a slot.
//!
//! No code yet: P0 scaffold only.

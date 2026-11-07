//! Input validation and post-solve verification for the DP.
//!
//! Slots arrive from the engine and the price store; a malformed or internally inconsistent slot
//! array must never reach the solver. After a solve the plan is re-derived from the returned
//! controls (SoC path, bounds, per-slot argmin) and compared against what the solver claims.
//!
//! No code yet: P0 scaffold only.

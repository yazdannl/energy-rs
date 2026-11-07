//! Capacity and efficiency learning.
//!
//! Solves `dSOC * C = I * x - O / x` over charge/discharge sessions with a 3-session half-life,
//! rejects outliers, applies a ±25 % band around the pack label, and never estimates above the
//! reported capacity. Each estimate carries its provenance for the UI.
//!
//! No code yet: P0 scaffold only.

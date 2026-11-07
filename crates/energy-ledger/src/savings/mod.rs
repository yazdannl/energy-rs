//! Savings accounting: one row per local day, derived from measured integrals.
//!
//! P6 stores the daily row and its provenance (coverage, completeness); the per-slot actual vs
//! baseline calculation lands with the engine/server phases. Money is EUR and rounded to 1e-6 when
//! written.

mod daily;
mod repo;

pub use daily::SavingsDaily;
pub use repo::Savings;

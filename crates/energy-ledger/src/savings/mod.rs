//! Savings accounting: one row per local day, derived from measured integrals.
//!
//! This crate only stores the row and its provenance (coverage, completeness, the four-way money
//! split). The measurement, the counterfactual and the rollup live in `energy-core::savings`; see
//! `docs/SAVINGS.md`. Money is EUR and rounded to 1e-6 when written; a closed day whose slots were
//! not all measured stores `NULL` money instead of a number.

mod daily;
mod repo;

pub use daily::SavingsDaily;
pub use repo::Savings;

//! Brute-force reference DP for the differential checks (`PLAN.md` §6).
//!
//! Deliberately slow and self-contained: it builds its own state grid, sweeps every state with a
//! fresh action list, and reconstructs the path in the most direct way possible, so that it shares
//! no arithmetic with the C core and no helper with [`crate::validate`].  A disagreement between
//! the two is therefore a real disagreement, not a shared typo.
//!
//! Used by the differential tests and, in the engine, by the periodic full differential; never on
//! the fast path (a solve here costs a few milliseconds, which the 15-minute replan does not need
//! on every tick).
//!
//! Split by stage -- `sample` (one slot), `windows` / `arithmetic` (the naive step math),
//! `grid` (the state grid), `sweep` (the backward pass), `replay` (the path) and `solve`
//! (the orchestration) -- so each file stays reviewable.

mod arithmetic;
mod grid;
mod replay;
mod result;
mod sample;
mod solve;
mod sweep;
mod windows;

pub use result::OracleResult;
pub use solve::solve;

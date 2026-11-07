//! Dynamic-programming optimizer core.
//!
//! Responsibility (PLAN.md §2 and §6): own the DP solver and everything that keeps it honest.
//!
//! * The C core (`_core.c` in the old add-on) is vendored byte-identically and built with the old
//!   flags; it stays the single source of the optimization result.
//! * [`dp`] is the safe Rust wrapper around `solve(pv[], load[], buy[], sell[], minutes[],
//!   config[, terminal_row])`.
//! * [`validate`] owns input validation and post-solve checks: feasibility, bounds, SoC path
//!   re-derivation and per-slot argmin decision equality. A plan that fails any check is refused,
//!   never "used because it is only slightly wrong".
//! * [`oracle`] holds a small, obviously-correct brute-force DP used for differential testing.
//!
//! Everything here is synchronous and allocation-light: a solve happens every 15 minutes on a
//! 120-slot horizon and must stay well under the 50 ms budget.

pub mod dp;
pub mod oracle;
pub mod validate;

/// Version of the vendored C DP core.
///
/// P0 does not vendor `_core.c` yet; P1 replaces this with a value emitted by `build.rs`.
pub const CORE_VERSION: &str = "unvendored";

//! The safe Rust API around the vendored C DP solver.
//!
//! [`solve`] marshals a scenario and a [`Config`](crate::config::Config) into the flat C ABI,
//! copies the result out and then runs [`crate::validate::check`] on it.  The returned
//! [`SolveResult`] mirrors the dict the old CPython extension returned, field for field, so a call
//! site can be compared with the add-on's.
//!
//! Every failure is typed ([`SolveError`]) and every failure is a refusal: there is no "use the
//! plan anyway" path (`PLAN.md` §6).

mod identity;
mod input;
mod result;
mod solve;
mod window;

pub use identity::{core_build_sha, verify_core_build};
pub use input::SolveInput;
pub use result::SolveResult;
pub use solve::{solve, solve_with_terminal_row};
pub use window::Window;

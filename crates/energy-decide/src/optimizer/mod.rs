//! M2: pure-Rust optimizer objective, hard constraints and canonical tie-break.
//!
//! This module never links, binds or calls the legacy C solver. It takes typed M1-style slot
//! inputs and returns either a deterministic schedule with every objective term or a typed
//! refusal; M3 remains responsible for the assembled plan state and dispatch permission.

pub mod actions;
pub mod binding_bound;
pub mod charge_limit;
pub mod choose;
pub mod config;
pub mod context;
pub mod continuation;
pub mod curtailment;
pub mod dp;
pub mod feasible;
pub mod finish;
pub mod input;
pub mod key;
pub mod lattice;
pub mod objective;
pub mod objective_breakdown;
pub mod pack_allocation;
pub mod path;
pub mod refusal;
pub mod repair;
pub mod result;
pub mod schedule_slot;
pub mod shared;
pub mod slot;
pub mod tie_break;
pub mod validate;

pub use actions::ActionGrid;
pub use binding_bound::BindingBound;
pub use charge_limit::max_charge_w_at_soc;
pub use config::OptimizerConfig;
pub use curtailment::{Curtailment, solve as solve_curtailment};
pub use dp::solve;
pub use input::OptimizerInput;
pub use lattice::StateLattice;
pub use objective_breakdown::ObjectiveBreakdown;
pub use pack_allocation::{PackAllocation, allocate_pack_powers};
pub use refusal::OptimizerRefusal;
pub use result::OptimizerResult;
pub use schedule_slot::ScheduleSlot;
pub use slot::Slot;
pub use tie_break::TieBreak;

/// Short alias for the objective breakdown.
pub use objective_breakdown::ObjectiveBreakdown as Objective;

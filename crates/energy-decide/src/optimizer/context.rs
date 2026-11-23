//! Shared inputs for one DP decision, used by choice and continuation replay.

use crate::optimizer::OptimizerInput;
use crate::optimizer::continuation::Continuation;
use crate::optimizer::dp::ActionEval;
use crate::optimizer::shared::SolverShared;

pub(crate) struct ChooseContext<'a> {
    pub(crate) input: &'a OptimizerInput,
    pub(crate) index: usize,
    pub(crate) points: &'a [f64],
    pub(crate) evals_by_slot: &'a [Vec<ActionEval>],
    pub(crate) suffixes: &'a [Vec<Option<Continuation>>],
    pub(crate) salvage: f64,
    pub(crate) shared: &'a SolverShared,
}

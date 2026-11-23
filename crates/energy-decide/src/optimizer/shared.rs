//! State shared by all DP layers: continuation cache and precomputed rejection bounds.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::optimizer::BindingBound;
use crate::optimizer::continuation::Continuation;

pub(crate) struct SolverShared {
    pub(crate) cache: RefCell<HashMap<(usize, u64), Continuation>>,
    pub(crate) bounds: Vec<BindingBound>,
}

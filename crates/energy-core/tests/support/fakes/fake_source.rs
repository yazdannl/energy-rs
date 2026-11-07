//! A plan source that returns one fixed plan, or a typed refusal.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_possible_wrap
)]
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use energy_core::dispatch::BoxFuture;
use energy_core::engine::PlanSource;
use energy_core::plan::{PackState, Plan, PlanError};
use energy_core::replan::Changes;

/// A plan source that returns one fixed plan, or a typed refusal.
pub struct FakeSource {
    plan: Plan,
    refuse: AtomicBool,
    changes: Changes,
}

impl FakeSource {
    pub fn new(plan: Plan) -> Self {
        Self {
            plan,
            refuse: AtomicBool::new(false),
            changes: Changes::none(),
        }
    }

    pub fn refuse(&self, refuse: bool) {
        self.refuse.store(refuse, Ordering::Relaxed);
    }
}

impl PlanSource for FakeSource {
    fn plan(
        &self,
        _now: i64,
        _packs: Vec<energy_core::plan::PackState>,
    ) -> BoxFuture<'_, Result<Plan, PlanError>> {
        Box::pin(async move {
            if self.refuse.load(Ordering::Relaxed) {
                return Err(PlanError::NoPacks);
            }
            Ok(self.plan.clone())
        })
    }

    fn changes(&self) -> Changes {
        self.changes
    }
}

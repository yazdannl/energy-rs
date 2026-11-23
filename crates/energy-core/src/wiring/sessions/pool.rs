//! Fleet-wide capacity pool: membership follows evidence, not scheduling.

use energy_battery::learning::{CapacityBounds, CapacityPool, LearningProfile, SessionRecord};
use energy_ledger::Session;

use crate::engine::instant;

/// One pack's extracted window and the profile it must extend.
pub(super) struct PackState {
    pub(super) sn: String,
    pub(super) id: i64,
    pub(super) bounds: CapacityBounds,
    pub(super) sessions: Vec<Session>,
    pub(super) records: Vec<SessionRecord>,
    pub(super) complete: usize,
    pub(super) previous: Option<LearningProfile>,
}

/// Every pack with sessions contributes, regardless of skip state or relearn target.
pub(super) fn fleet_pool(states: &[PackState], now: i64) -> CapacityPool {
    let peers = states
        .iter()
        .map(|state| (state.bounds.label_kwh, state.records.as_slice()))
        .collect::<Vec<_>>();
    CapacityPool::from_sessions(&peers, instant(now))
}

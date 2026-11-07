//! Full-state updates emitted by subscriptions.

use crate::state::EntityState;

#[derive(Debug, Clone, PartialEq)]
pub enum EntityUpdate {
    Added(EntityState),
    Changed(EntityState),
    Removed(String),
}

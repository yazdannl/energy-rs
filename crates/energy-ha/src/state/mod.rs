//! Entity state shared by REST, history and live subscriptions.

mod entity_state;

pub(crate) use entity_state::deserialize_timestamp;
pub use entity_state::{EntityMap, EntityState};

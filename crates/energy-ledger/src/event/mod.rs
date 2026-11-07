//! The journal: append-only events with a level, a kind and an opaque JSON payload.

mod entry;
mod level;
mod repo;

pub use entry::Event;
pub use level::EventLevel;
pub use repo::Events;

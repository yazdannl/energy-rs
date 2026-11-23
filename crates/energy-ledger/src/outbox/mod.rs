//! Durable outbox for learning audit events.

mod entry;
mod repo;

pub use entry::PendingLearningEvent;
pub use repo::LearningOutbox;

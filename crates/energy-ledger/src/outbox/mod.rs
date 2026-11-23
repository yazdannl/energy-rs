//! Durable outbox for learning audit events.

mod entry;
mod outcome;
mod quarantine;
mod repo;

pub use entry::PendingLearningEvent;
pub use outcome::PublishOutcome;
pub use quarantine::QuarantinedEvent;
pub use repo::LearningOutbox;

#[cfg(test)]
mod repo_tests;

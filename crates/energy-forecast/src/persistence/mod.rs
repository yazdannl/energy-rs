//! The learning payload store: schema-versioned records, the trait and an in-memory impl.
//!
//! Storage itself belongs to the ledger crate. This crate only defines the record shape, refuses a
//! payload written by an older schema, and ships an in-memory store for tests and dry runs.

mod memory;
mod record;
mod store;

pub use memory::MemoryLearningStore;
pub use record::{CURRENT_SCHEMA_VERSION, LearningRecord};
pub use store::LearningStore;

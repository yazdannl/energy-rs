//! Storage port for the append-only startup-resolution journal.

use crate::authority::{StartupResolutionRecord, StorageError};

/// Append-only diagnostic record store with readback for anchor-reference checks.
pub trait ResolutionStore: Send + Sync {
    /// Next monotonic record id, without appending anything.
    fn next_id(&self) -> Result<u64, StorageError>;
    /// Append one record, fsync it and return its id.
    fn append(&self, record: &StartupResolutionRecord) -> Result<u64, StorageError>;
    /// Read a referenced record back; missing or corrupt is a typed error.
    fn read(&self, id: u64) -> Result<Option<StartupResolutionRecord>, StorageError>;
}

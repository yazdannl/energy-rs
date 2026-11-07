//! The store interface the ledger crate implements.

use super::record::LearningRecord;
use crate::ForecastError;

/// Read and write schema-versioned learning payloads.
///
/// Deliberately synchronous and small: the ledger owns SQLite, this crate owns only the payloads.
pub trait LearningStore {
    /// The record stored under `(kind, key)`, if any.
    fn get(&self, kind: &str, key: &str) -> Option<LearningRecord>;

    /// Store a record, refusing one written by a schema this build does not read.
    fn put(&mut self, record: LearningRecord) -> Result<(), ForecastError>;

    /// Remove a record; `true` when something was there.
    fn remove(&mut self, kind: &str, key: &str) -> bool;

    /// The records of one kind, ordered by key.
    fn list(&self, kind: &str) -> Vec<LearningRecord>;
}

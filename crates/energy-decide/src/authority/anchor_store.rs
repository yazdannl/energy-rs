//! Storage port for the separate-file anchor.

use crate::authority::{AuthorityAnchor, StorageError};

/// Reads and atomically replaces the authority anchor file.
pub trait AnchorStore: Send + Sync {
    /// Load the anchor, or `None` before first bootstrap.
    fn load(&self) -> Result<Option<AuthorityAnchor>, StorageError>;
    /// Replace the anchor atomically; no torn file is ever visible.
    fn atomic_write(&self, anchor: &AuthorityAnchor) -> Result<(), StorageError>;
}

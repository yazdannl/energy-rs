//! Real separate-file anchor store using atomic replacement.

use std::path::{Path, PathBuf};

use crate::authority::{AnchorStore, AuthorityAnchor, StorageError, atomic_file};

/// Anchor stored at `data/authority_anchor.json` in the wiring; tests pass a temp directory.
#[derive(Debug, Clone)]
pub struct FileAnchorStore {
    path: PathBuf,
}

impl FileAnchorStore {
    /// Anchor file path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Anchor inside an existing directory.
    pub fn in_dir(dir: &Path) -> Self {
        Self::new(dir.join("authority_anchor.json"))
    }

    /// Exposed for fault injection tests that corrupt the file outside the engine.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AnchorStore for FileAnchorStore {
    fn load(&self) -> Result<Option<AuthorityAnchor>, StorageError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let contents = std::fs::read_to_string(&self.path).map_err(|_| StorageError::Corrupt)?;
        let anchor = serde_json::from_str(&contents).map_err(|_| StorageError::Corrupt)?;
        Ok(Some(anchor))
    }

    fn atomic_write(&self, anchor: &AuthorityAnchor) -> Result<(), StorageError> {
        let contents = serde_json::to_string(anchor).map_err(|_| StorageError::Corrupt)?;
        atomic_file::replace(&self.path, &contents)
    }
}

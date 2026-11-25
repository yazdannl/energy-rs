//! Real append-only startup-resolution record with file and first-creation directory fsync.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::authority::{ResolutionStore, StartupResolutionRecord, StorageError, atomic_file};

/// JSONL store; the engine never rewrites or truncates an existing record.
#[derive(Debug, Clone)]
pub struct FileResolutionStore {
    path: PathBuf,
}

impl FileResolutionStore {
    /// Record file path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Record inside an existing data directory.
    pub fn in_dir(dir: &Path) -> Self {
        Self::new(dir.join("startup_resolution.jsonl"))
    }

    /// Exposed for fault injection tests that delete or corrupt a referenced record.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn records(&self) -> Result<Vec<StartupResolutionRecord>, StorageError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let contents = std::fs::read_to_string(&self.path).map_err(|_| StorageError::Corrupt)?;
        contents
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(line).map_err(|_| StorageError::Corrupt))
            .collect()
    }
}

impl ResolutionStore for FileResolutionStore {
    fn next_id(&self) -> Result<u64, StorageError> {
        Ok(self
            .records()?
            .last()
            .map_or(1, |record| record.id.saturating_add(1)))
    }

    fn append(&self, record: &StartupResolutionRecord) -> Result<u64, StorageError> {
        let first_creation = !self.path.exists();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::PermissionDenied {
                    StorageError::ReadOnly
                } else if error.kind() == std::io::ErrorKind::StorageFull {
                    StorageError::StorageFull
                } else {
                    StorageError::FsyncFailed
                }
            })?;
        let line = serde_json::to_string(record).map_err(|_| StorageError::Corrupt)?;
        file.write_all(format!("{line}\n").as_bytes())
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::StorageFull {
                    StorageError::StorageFull
                } else {
                    StorageError::FsyncFailed
                }
            })?;
        file.sync_all().map_err(|_| StorageError::FsyncFailed)?;
        if first_creation {
            atomic_file::sync_dir(&self.path)?;
        }
        Ok(record.id)
    }

    fn read(&self, id: u64) -> Result<Option<StartupResolutionRecord>, StorageError> {
        Ok(self.records()?.into_iter().find(|record| record.id == id))
    }
}

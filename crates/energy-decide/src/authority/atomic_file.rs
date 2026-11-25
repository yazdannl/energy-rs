//! Temp file, fsync, rename, parent-directory fsync. No torn anchor is ever observable.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::authority::StorageError;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn io_error(error: &std::io::Error, dir: bool) -> StorageError {
    match error.kind() {
        std::io::ErrorKind::StorageFull => StorageError::StorageFull,
        std::io::ErrorKind::PermissionDenied => StorageError::ReadOnly,
        _ if dir => StorageError::DirFsyncFailed,
        _ => StorageError::FsyncFailed,
    }
}

pub(crate) fn sync_dir(path: &Path) -> Result<(), StorageError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|error| io_error(&error, true))
}

/// Atomically replace one file and fsync both the file and its parent directory.
pub(crate) fn replace(path: &Path, contents: &str) -> Result<(), StorageError> {
    let sequence = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
    let result = write_temp(&temp, contents).and_then(|()| {
        std::fs::rename(&temp, path).map_err(|error| io_error(&error, false))?;
        sync_dir(path)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn write_temp(path: &Path, contents: &str) -> Result<(), StorageError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io_error(&error, false))?;
    file.write_all(contents.as_bytes())
        .map_err(|error| io_error(&error, false))?;
    file.sync_all().map_err(|error| io_error(&error, false))
}

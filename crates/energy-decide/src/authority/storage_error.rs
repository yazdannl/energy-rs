//! Storage failures mapped to the seven typed startup-resolution reasons.

/// Failure while durably writing or reading authority storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    /// Disk full.
    StorageFull,
    /// Read-only filesystem or directory.
    ReadOnly,
    /// File fsync failed.
    FsyncFailed,
    /// Parent-directory fsync failed.
    DirFsyncFailed,
    /// Corrupt append or readback.
    Corrupt,
    /// Atomic pending-reference write failed.
    PendingRefFailed,
    /// Promotion of the pending reference failed.
    PromotionFailed,
}

impl StorageError {
    /// Stable typed reason required by S220.
    pub fn slug(self) -> &'static str {
        match self {
            Self::StorageFull => "resolution_record_storage_full",
            Self::ReadOnly => "resolution_record_read_only",
            Self::FsyncFailed => "resolution_record_fsync_failed",
            Self::DirFsyncFailed => "resolution_dir_fsync_failed",
            Self::Corrupt => "resolution_record_corrupt",
            Self::PendingRefFailed => "anchor_ref_pending_failed",
            Self::PromotionFailed => "anchor_ref_promotion_failed",
        }
    }
}

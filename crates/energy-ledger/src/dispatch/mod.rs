//! The dispatch log: every requested and applied write, including dry runs and rejections.

mod entry;
mod repo;

pub use entry::DispatchEntry;
pub use repo::DispatchLog;

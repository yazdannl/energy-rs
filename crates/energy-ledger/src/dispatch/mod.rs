//! The dispatch log: every requested and applied write, including dry runs and rejections.

mod entry;
mod measured;
mod repo;

pub use entry::DispatchEntry;
pub use measured::MeasuredWatts;
pub use repo::DispatchLog;

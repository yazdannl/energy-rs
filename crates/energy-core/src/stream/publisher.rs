//! The write side of the engine/server contract.

use tokio::sync::watch;

use crate::stream::EngineSnapshot;

/// Publish the engine's read model to the HTTP layer.
///
/// The trait exists so the server can be built and tested without an engine loop: a test publishes
/// snapshots into a [`WatchPublisher`], P8b's loop publishes into the same one. Publishing must
/// never block the engine, which is why the tokio implementation is a `watch` channel (latest
/// value wins, slow readers are dropped, no unbounded queue).
pub trait SnapshotPublisher: Send + Sync {
    /// Replace the current snapshot.
    fn publish(&self, snapshot: EngineSnapshot);

    /// The current snapshot (a clone; snapshots are small plain values).
    fn current(&self) -> EngineSnapshot;

    /// A reader that is woken on every publish.
    fn subscribe(&self) -> watch::Receiver<EngineSnapshot>;
}

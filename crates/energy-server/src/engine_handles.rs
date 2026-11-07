//! The engine side of the server handshake: the snapshot publisher and the command receiver.
//!
//! P8b's engine loop takes these two handles from [`Service::new`](crate::Service::new), publishes
//! a snapshot after every tick and drains the commands the API accepted. Until then `main` drops
//! the receiver, so a write command is refused with `503 engine_unavailable` instead of being
//! silently accepted.

use energy_core::safety::Authority;
use energy_core::stream::{EngineCommand, EngineSnapshot, SnapshotPublisher, WatchPublisher};
use tokio::sync::mpsc;

/// What the engine keeps after the server is built.
#[derive(Debug)]
pub struct EngineHandles {
    /// Publish after every tick (see [`SnapshotPublisher`]).
    pub snapshots: WatchPublisher,
    /// Commands the API queued; the engine drains this and applies them.
    pub commands: mpsc::Receiver<EngineCommand>,
}

impl EngineHandles {
    pub(crate) fn new(snapshots: WatchPublisher, commands: mpsc::Receiver<EngineCommand>) -> Self {
        Self {
            snapshots,
            commands,
        }
    }

    /// Publish a snapshot (a convenience for the loop's first tick).
    pub fn publish(&self, snapshot: EngineSnapshot) {
        self.snapshots.publish(snapshot);
    }

    /// Publish the current mode without touching anything else.
    pub fn publish_mode(&self, mode: Authority) {
        let mut snapshot = self.snapshots.current();
        snapshot.mode = mode;
        self.snapshots.publish(snapshot);
    }
}

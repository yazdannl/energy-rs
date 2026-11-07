//! A seeded snapshot plus a stream of full-state updates.

use tokio::sync::mpsc;

use super::{EntityStore, EntityUpdate};

pub struct EntityWatch {
    pub snapshot: EntityStore,
    updates: mpsc::Receiver<EntityUpdate>,
}

impl EntityWatch {
    pub(crate) fn new(snapshot: EntityStore, updates: mpsc::Receiver<EntityUpdate>) -> Self {
        Self { snapshot, updates }
    }

    pub async fn next(&mut self) -> Option<EntityUpdate> {
        self.updates.recv().await
    }
}

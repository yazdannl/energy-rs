//! One subscription's event stream.

use std::{sync::Arc, time::Duration};

use serde_json::{Value, json};
use tokio::{sync::mpsc, time::timeout};

use super::{HaError, connection::Shared};

/// One subscription's event stream. Dropping it unsubscribes on a best-effort basis.
pub struct EventStream {
    shared: Arc<Shared>,
    id: u64,
    receiver: mpsc::Receiver<Value>,
}

impl EventStream {
    pub(crate) fn new(shared: Arc<Shared>, id: u64, receiver: mpsc::Receiver<Value>) -> Self {
        Self {
            shared,
            id,
            receiver,
        }
    }

    pub fn subscription_id(&self) -> u64 {
        self.id
    }

    /// Next `event` payload, or `None` after a disconnect/unsubscribe.
    pub async fn recv(&mut self) -> Option<Value> {
        self.receiver.recv().await
    }

    /// Next payload bounded by `duration`.
    pub async fn recv_timeout(&mut self, duration: Duration) -> Result<Option<Value>, HaError> {
        timeout(duration, self.receiver.recv())
            .await
            .map_err(|_| HaError::Timeout(duration))
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        self.shared.remove_subscription(self.id);
        let id = self.shared.next_id();
        let unsubscribe = json!({
            "id": id,
            "type": "unsubscribe_events",
            "subscription": self.id,
        });
        if let Ok(text) = serde_json::to_string(&unsubscribe) {
            self.shared.try_send_text(text);
        }
    }
}

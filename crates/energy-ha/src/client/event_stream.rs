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
        // The unsubscribe takes an id like any other command, so it goes through the send order
        // instead of racing an in-flight request; `drop` cannot await, so it spawns.
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let shared = Arc::clone(&self.shared);
        let subscription = self.id;
        handle.spawn(async move {
            let _order = shared.lock_send().await;
            let id = shared.next_id();
            let unsubscribe = json!({
                "id": id,
                "type": "unsubscribe_events",
                "subscription": subscription,
            });
            let _ = shared.send_value(&unsubscribe).await;
        });
    }
}

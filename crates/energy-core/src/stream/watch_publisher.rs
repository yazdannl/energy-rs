//! The tokio `watch` implementation of [`SnapshotPublisher`].

use tokio::sync::watch;

use crate::stream::{EngineSnapshot, SnapshotPublisher};

/// A publish/subscribe pair over one `watch` channel.
///
/// `send_replace` is used on purpose: the channel must accept a publish even when no reader is
/// subscribed (a resting UI), and the newest snapshot is the only one worth keeping. Readers see
/// every change through [`SnapshotPublisher::subscribe`]; a reader that falls behind is woken once
/// and sees the latest value, never a backlog.
#[derive(Debug, Clone)]
pub struct WatchPublisher {
    sender: watch::Sender<EngineSnapshot>,
}

impl WatchPublisher {
    /// Start with `initial` as the current snapshot.
    pub fn new(initial: EngineSnapshot) -> Self {
        Self {
            sender: watch::channel(initial).0,
        }
    }

    /// A publisher whose snapshot is [`EngineSnapshot::default`].
    pub fn empty() -> Self {
        Self::new(EngineSnapshot::default())
    }
}

impl SnapshotPublisher for WatchPublisher {
    fn publish(&self, snapshot: EngineSnapshot) {
        self.sender.send_replace(snapshot);
    }

    fn current(&self) -> EngineSnapshot {
        self.sender.borrow().clone()
    }

    fn subscribe(&self) -> watch::Receiver<EngineSnapshot> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    // A failed expectation is the test's report: `expect` is the intended API here.
    #![allow(clippy::expect_used)]

    use super::WatchPublisher;
    use crate::safety::Authority;
    use crate::stream::{EngineSnapshot, SnapshotPublisher};

    #[test]
    fn publishing_replaces_the_current_snapshot() {
        let publisher = WatchPublisher::empty();
        assert_eq!(publisher.current().mode, Authority::DryRun);

        let next = EngineSnapshot {
            mode: Authority::Auto,
            ..EngineSnapshot::default()
        };
        publisher.publish(next.clone());
        assert_eq!(publisher.current(), next);
    }

    #[tokio::test]
    async fn a_subscriber_is_woken_by_a_publish() {
        let publisher = WatchPublisher::empty();
        let mut reader = publisher.subscribe();

        let next = EngineSnapshot {
            last_tick: Some(1_700_000_000),
            ..EngineSnapshot::default()
        };
        publisher.publish(next);

        reader.changed().await.expect("the publisher is alive");
        assert_eq!(reader.borrow().last_tick, Some(1_700_000_000));
    }

    #[tokio::test]
    async fn a_publish_without_readers_still_updates_the_current_value() {
        let publisher = WatchPublisher::empty();
        drop(publisher.subscribe());
        let next = EngineSnapshot {
            mode: Authority::Advisory,
            ..EngineSnapshot::default()
        };
        publisher.publish(next);
        assert_eq!(publisher.current().mode, Authority::Advisory);
    }
}

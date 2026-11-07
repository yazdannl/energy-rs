//! The read model every handler shares.
//!
//! The server owns two data sources: the ledger (history: stored plans, dispatch rows, savings,
//! settings, pack identity) and the price store (published slots). Everything live comes from the
//! engine's [`EngineSnapshot`] through a `watch` channel, so a read request never talks to HA or a
//! battery.
//!
//! SQLite calls are synchronous (`rusqlite`), so the API runs them on the blocking pool
//! (`api::blocking`); this struct only holds the handles.

use std::sync::Arc;
use std::time::Duration;

use energy_core::safety::Authority;
use energy_core::stream::SnapshotPublisher;
use energy_core::stream::{EngineCommand, EngineSnapshot, WatchPublisher};
use energy_ledger::Ledger;
use energy_price::{PriceRepository, Settlement};
use tokio::sync::{Notify, mpsc, watch};

use crate::engine_handles::EngineHandles;
use crate::service_options::ServiceOptions;

/// How many API commands may wait for the engine before the queue is considered full.
const COMMAND_QUEUE: usize = 32;

/// The shared application state handed to every route.
pub struct Service {
    ledger: Arc<Ledger>,
    prices: Arc<dyn PriceRepository>,
    settlement: Arc<Settlement>,
    snapshots: WatchPublisher,
    commands: mpsc::Sender<EngineCommand>,
    heartbeat: Duration,
    refresh: Arc<Notify>,
}

impl Service {
    /// Build the read model and the two engine handles.
    ///
    /// The returned [`EngineHandles`] are the engine's side of the contract. Dropping them is valid:
    /// reads keep working and every write command answers `503 engine_unavailable` until an engine
    /// loop drains the queue.
    pub fn new(
        ledger: Arc<Ledger>,
        prices: Arc<dyn PriceRepository>,
        settlement: Settlement,
        options: ServiceOptions,
    ) -> (Arc<Self>, EngineHandles) {
        let snapshots = WatchPublisher::empty();
        let (commands, command_rx) = mpsc::channel(COMMAND_QUEUE);
        let service = Arc::new(Self {
            ledger,
            prices,
            settlement: Arc::new(settlement),
            snapshots: snapshots.clone(),
            commands,
            heartbeat: options.heartbeat,
            refresh: Arc::new(Notify::new()),
        });
        (service, EngineHandles::new(snapshots, command_rx))
    }

    /// The ledger handle (history reads and settings writes).
    pub fn ledger(&self) -> &Arc<Ledger> {
        &self.ledger
    }

    /// The price repository (published slots).
    pub fn prices(&self) -> &Arc<dyn PriceRepository> {
        &self.prices
    }

    /// The settlement contract used to derive the feed-in price per slot.
    pub fn settlement(&self) -> &Settlement {
        &self.settlement
    }

    /// The newest engine snapshot.
    pub fn snapshot(&self) -> EngineSnapshot {
        self.snapshots.current()
    }

    /// The mode the engine last published.
    pub fn mode(&self) -> Authority {
        self.snapshot().mode
    }

    /// A reader woken on every publish (used by the SSE stream).
    pub fn subscribe(&self) -> watch::Receiver<EngineSnapshot> {
        self.snapshots.subscribe()
    }

    /// Ask the load-profile loop to relearn now (a setting changed).
    pub fn request_profile_refresh(&self) {
        self.refresh.notify_one();
    }

    /// The signal the load-profile loop waits on.
    pub fn profile_refresh(&self) -> Arc<Notify> {
        Arc::clone(&self.refresh)
    }

    /// The configured SSE heartbeat interval.
    pub fn heartbeat(&self) -> Duration {
        self.heartbeat
    }

    /// Queue one command for the engine.
    pub(crate) fn send_command(&self, command: EngineCommand) -> Result<(), CommandRefusal> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CommandRefusal::Busy,
                mpsc::error::TrySendError::Closed(_) => CommandRefusal::Stopped,
            })
    }
}

/// Why a command could not be queued; the API turns this into `503 engine_unavailable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandRefusal {
    /// The engine is alive but not draining fast enough.
    Busy,
    /// No engine is running (P8b not started, or the loop stopped).
    Stopped,
}

impl CommandRefusal {
    /// A short, non-sensitive explanation for the error details.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Busy => "command queue is full",
            Self::Stopped => "no engine loop is running",
        }
    }
}

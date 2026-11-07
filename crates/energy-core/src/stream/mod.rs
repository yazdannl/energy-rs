//! The engine/server contract: a read-only snapshot the engine publishes, and the commands the
//! HTTP layer sends back.
//!
//! The engine loop (P8b) owns every decision; the server owns nothing but rendering. The two meet
//! through this module:
//!
//! * [`EngineSnapshot`] — everything the API needs to answer a read request: mode, current plan,
//!   per-pack state (with the learned profile and the protection latch), meter, forecast, the last
//!   tick and the open faults. It is a plain value, so publishing it never blocks the loop.
//! * [`SnapshotPublisher`] — the write side of the contract. [`WatchPublisher`] is the tokio
//!   `watch` implementation: readers always see the newest snapshot and a slow reader never queues
//!   up work for the engine.
//! * [`EngineCommand`] — the few writes the API may request (`mode`, a manual pack override, a
//!   full charge, a relearn pass). The server only reports that the engine accepted them; the
//!   effect becomes visible through the next snapshot. Overrides use the engine's own
//!   [`dispatch::OverrideAction`](crate::dispatch::OverrideAction), so there is one spelling of
//!   `charge`/`hold`/`off` in the whole system.
//!
//! Timestamps are UTC epoch seconds; the HTTP layer formats them as RFC3339.

pub mod command;
pub mod fault_snapshot;
pub mod forecast_snapshot;
pub mod meter_snapshot;
pub mod override_command;
pub mod pack_snapshot;
pub mod plan_snapshot;
pub mod publisher;
pub mod runtime_view;
pub mod site_snapshot;
pub mod site_source;
pub mod snapshot;
pub mod watch_publisher;

pub use command::EngineCommand;
pub use fault_snapshot::FaultSnapshot;
pub use forecast_snapshot::ForecastSnapshot;
pub use meter_snapshot::MeterSnapshot;
pub use override_command::OverrideCommand;
pub use pack_snapshot::PackSnapshot;
pub use plan_snapshot::PlanSnapshot;
pub use publisher::SnapshotPublisher;
pub use runtime_view::RuntimeView;
pub use site_snapshot::SiteSnapshot;
pub use site_source::SiteSource;
pub use snapshot::EngineSnapshot;
pub use watch_publisher::WatchPublisher;

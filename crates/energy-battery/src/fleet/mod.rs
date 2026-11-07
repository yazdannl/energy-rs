//! A fleet of devices: staggered polling, a freshness-stamped cache, aggregation and fan-out.
//!
//! Four batteries are four HTTP hosts with no batch endpoint, so the fleet loops. Polls are
//! staggered (default one device every 5 s, offset by index) so a tick never asks every device at
//! once, writes fan out in parallel across devices while each device keeps its own ≥2 s spacing, and
//! a device that fails is reported as a partial failure instead of aborting the others.

pub mod aggregate;
pub mod client;
pub mod device_snapshot;
pub mod fanout;
pub mod outcome;
pub mod snapshot;

pub use aggregate::FleetAggregate;
pub use client::{DEFAULT_POLL_INTERVAL, FleetClient};
pub use device_snapshot::DeviceSnapshot;
pub use fanout::FanOutReport;
pub use outcome::DeviceOutcome;
pub use snapshot::FleetSnapshot;

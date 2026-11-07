//! Battery fleet: discovery, control, learning and protection.
//!
//! Responsibility (PLAN.md §2, §5): own the SolarFlow packs over the local zenSDK HTTP API. No
//! cloud, no token, no TLS.
//!
//! * [`transport`] — plain HTTP on port 80, one host per device: `GET /properties/report` and
//!   `POST /properties/write` with a 512-byte body cap. A write only counts as applied when the
//!   next report echoes the limit; HTTP 200 plus `code: 200` proves nothing.
//! * [`report`] — tolerant parser for both report envelopes and the unit decoders that turn raw
//!   integers into volts, amps, percent and °C. Pack capacity labels come from the pack serial
//!   prefix plus `packType`; the report carries no kWh counter at all.
//! * [`control`] — command builder (charge / discharge / RAM hold / trim), clamping to the config
//!   and the device's own maxima, ≥2 s write pacing with one command in flight, echo verification
//!   on the next report, and wedge/restart detection. A hold never writes `acMode`, every control
//!   write re-asserts `smartMode: 1` (RAM, not flash).
//! * [`discovery`] — mDNS (`_zendure._tcp`) plus manual IP + SN entry, merged and deduped by serial.
//! * [`fleet`] — N devices: staggered polling, a state cache with freshness stamps, capacity
//!   weighted SoC and summed AC power, writes fanned out with per-device sequencing and partial
//!   failure reporting.
//! * [`learning`] — per-pack capacity and round-trip efficiency from charge/discharge sessions.
//! * [`protection`] — latching low-cell guard and the weekly full-charge maintenance.
//! * [`fake`] — an in-process zenSDK device (loopback HTTP) for tests and shadow runs. The crate
//!   never talks to real hardware unless the caller points a transport at a device.
//!
//! Protocol facts and their evidence tags live in `reports/energy-rs/zensdk.md`; the crate README
//! lists what is still unverified on hardware.

pub mod control;
pub mod discovery;
pub mod error;
pub mod fake;
pub mod fleet;
pub mod learning;
pub mod protection;
pub mod report;
pub mod transport;

pub use control::{
    Applied, Command, ControlFault, ControlLimits, DeviceController, Direction, ECHO_TOLERANCE_W,
    FAULT_BACKOFF, MIN_WRITE_SPACING, Observation, Pacer, clamp_command, clamp_watts,
};
pub use discovery::{
    DiscoveredDevice, DiscoveryRegistry, DiscoverySource, browse_mdns, build_hostname,
    parse_hostname,
};
pub use error::BatteryError;
pub use fake::{FakeBehaviour, FakeDevice, FakePack, FakeState};
pub use fleet::{DeviceSnapshot, FanOutReport, FleetAggregate, FleetClient, FleetSnapshot};
pub use report::{GridReverse, PackLabel, PackReport, Properties, Report, SocLimit, parse_report};
pub use transport::{
    BoxFuture, DeviceIo, Endpoint, HttpTransport, MAX_BODY_BYTES, WriteAck, WriteProperties,
    WriteRequest,
};

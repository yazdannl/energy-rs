//! An in-process zenSDK device: loopback HTTP, programmable state, no hardware.
//!
//! The fake answers the two real endpoints on `127.0.0.1:0`, echoes applied limits, drops writes
//! that follow another within 1.3 s (the measured device behaviour), can wedge its `acMode`, and can
//! inject host names, HTTP statuses and body codes. Tests drive it through the real
//! [`HttpTransport`](crate::HttpTransport), so the wire format is exercised end to end.

pub mod behaviour;
pub mod device;
pub mod pack;
pub mod report_json;
pub mod state;
pub mod write;

pub use behaviour::FakeBehaviour;
pub use device::FakeDevice;
pub use pack::FakePack;
pub use state::FakeState;

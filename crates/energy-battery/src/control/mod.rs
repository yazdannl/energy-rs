//! Control plane: commands, clamping, pacing, echo confirmation and device faults.
//!
//! The rules that shape this module come from measurements, not preference:
//!
//! * a write is 1–4 properties in one body, and HTTP 200 proves nothing — the *next report* must
//!   echo the limit within 5 W;
//! * two writes to one device are ≥2 s apart and only one is in flight (faster loops drop commands
//!   and have wedged `acMode` outright);
//! * `smartMode: 1` (RAM, not flash) is re-asserted on every control write;
//! * a hold is both limits at 0 and never writes `acMode`;
//! * `acMode` outside {1, 2} means the AC path is wedged: stop writing and surface a fault;
//! * a `messageId` that went backwards means the device rebooted: re-read before commanding.

pub mod applied;
pub mod clamp;
pub mod command;
pub mod controller;
pub mod direction;
pub mod echo;
pub mod fault;
pub mod hard_limit;
pub mod limits;
pub mod observation;
pub mod pace_claim;
pub mod pacer;
pub mod pending;
pub(super) mod state;
pub mod watch;

pub use applied::Applied;
pub use clamp::{Ceiling, ceiling_for, clamp_command, clamp_watts};
pub use command::Command;
pub use controller::{DeviceController, FAULT_BACKOFF, MAX_EMERGENCY_HOLDS};
pub use direction::Direction;
pub use echo::{ECHO_TOLERANCE_W, verify};
pub use fault::ControlFault;
pub use hard_limit::{HardLimit, MAX_HARD_LIMIT_W};
pub use limits::ControlLimits;
pub use observation::Observation;
pub use pace_claim::PaceClaim;
pub use pacer::{MIN_WRITE_SPACING, Pacer};
pub use pending::Pending;

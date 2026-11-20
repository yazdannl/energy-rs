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
//! * a `messageId` that went backwards is only a proven reboot when the clock moved forward;
//!   otherwise it is inconclusive, never a confirmation or a fault;
//! * a fresh report can still show the old limits, so a write is confirmed inside a hard
//!   [`ECHO_CONFIRM_WINDOW`] bound, and an ordered, unwrapped counter makes that comparison wrap-safe;
//! * a neutralising hold has safety priority: it preempts an unresolved write after
//!   [`SAFETY_HOLD_WINDOW`], so cell-voltage, emergency and shutdown holds cannot be blocked.
//!
//! # Heartbeat wiring
//!
//! [`DeviceController::poll_confirming`] marks progress while it waits *and while a read is in
//! flight*. energy-core's `BatterySink::send` now calls it with
//! `|| progress.mark(now)`; `Dispatcher::send` passes the tick's progress through
//! `CommandSink::write_with_progress`/`hold_with_progress`. The same path serves the setpoint write
//! and its one fail-safe hold.

pub mod applied;
pub mod clamp;
pub mod command;
mod confirmation;
pub mod controller;
pub mod direction;
pub mod echo;
mod emergency;
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
pub use controller::{DeviceController, FAULT_BACKOFF, MAX_EMERGENCY_HOLDS, SAFETY_HOLD_WINDOW};
pub use direction::Direction;
pub use echo::{ECHO_CONFIRM_WINDOW, ECHO_POLL_INTERVAL, ECHO_TOLERANCE_W, is_echo_doubt, verify};
pub use fault::ControlFault;
pub use hard_limit::{HardLimit, MAX_HARD_LIMIT_W};
pub use limits::ControlLimits;
pub use observation::Observation;
pub use pace_claim::PaceClaim;
pub use pacer::{MIN_WRITE_SPACING, Pacer};
pub use pending::Pending;

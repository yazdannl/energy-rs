//! Dispatcher: one tick of the live control loop.
//!
//! A tick reads the fleet and the grid meter through the [`DeviceFleet`] and [`MeterSource`] ports,
//! takes the plan slot in force, interpolates its setpoint over the first
//! [`TRANSITION_SECONDS`] of the slot, splits it across the packs, adds the fleet-level grid check,
//! runs the ordered safety ladder per pack and emits the result through a [`CommandSink`]. Every
//! pack gets a [`PackReport`] whether it was written, clamped, held, skipped or refused, so "why
//! didn't it charge?" always has an answer.
//!
//! Nothing here decides authority: [`TickInput::mode`] is the gate, `advisory`/`dry_run` run the
//! whole ladder in shadow, log the intended write and send nothing at all -- not even a hold.
//! Staleness is also decided above: [`TickInput::stale`] makes the tick hold instead of keeping the
//! last setpoint (survey §7 trap 4).
//!
//! The real implementations of the two ports live here too: [`BatteryFleet`] wraps the fleet cache
//! from `energy-battery` and [`BatterySink`] wraps its per-device controllers (pacing and echo
//! included). Tests use fakes that answer synchronously.

mod battery_fleet;
mod battery_sink;
mod command_sink;
mod device_fleet;
mod dispatcher;
mod fault_guard;
mod fleet_error;
mod fleet_grid;
mod fleet_view;
mod manual_override;
mod meter_reading;
mod meter_source;
mod override_action;
mod override_source;
mod overrides;
mod pack_context;
mod pack_reading;
mod pack_report;
mod progress;
mod sink_error;
mod slot_power;
mod tick_input;
mod tick_outcome;
mod write_receipt;

pub use battery_fleet::BatteryFleet;
pub use battery_sink::BatterySink;
pub use command_sink::CommandSink;
pub use device_fleet::DeviceFleet;
pub use dispatcher::Dispatcher;
pub use fault_guard::FaultGuard;
pub use fleet_error::FleetError;
pub use fleet_view::FleetView;
pub use manual_override::ManualOverride;
pub use meter_reading::MeterReading;
pub use meter_source::MeterSource;
pub use override_action::OverrideAction;
pub use override_source::OverrideSource;
pub use overrides::Overrides;
pub use pack_reading::PackReading;
pub use pack_report::PackReport;
pub use progress::Progress;
pub use sink_error::SinkError;
pub use slot_power::slot_power;
pub use tick_input::TickInput;
pub use tick_outcome::TickOutcome;
pub use write_receipt::WriteReceipt;

/// A boxed future, so the ports stay object-safe without `async fn` in traits.
pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Seconds after a slot boundary over which the setpoint is interpolated (survey §7 trap 17).
pub const TRANSITION_SECONDS: i64 = 120;
/// A setpoint at or below this many watts is sent as a hold, never as a tiny charge.
pub const HOLD_DEADBAND_W: f64 = 30.0;

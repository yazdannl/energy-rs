//! The grid-meter port.

use crate::dispatch::BoxFuture;
use crate::dispatch::meter_reading::MeterReading;

/// Where the grid and site flows come from.
///
/// Implementations must not invent numbers: a meter that cannot answer returns a reading whose
/// fields are `None`, which the ladder treats as missing telemetry (fail closed).
pub trait MeterSource: Send + Sync {
    /// Read the meter as of this tick.
    fn reading(&self) -> BoxFuture<'_, MeterReading>;
}

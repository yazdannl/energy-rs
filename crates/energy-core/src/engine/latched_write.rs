//! One persisted live setpoint.

/// A setpoint this process (or a previous one) put on a device and has not seen neutralised.
///
/// `boot` marks an entry that was restored from the ledger, i.e. written by an earlier process:
/// only those may be neutralised at boot, which is what keeps a fresh advisory install from
/// touching hardware it never drove (survey §7 trap 2).
#[derive(Debug, Clone, PartialEq)]
pub struct LatchedWrite {
    /// The pack we wrote to.
    pub pack_id: String,
    /// The setpoint we left on it, W (positive charging).
    pub power_w: f64,
    /// When it was written, UTC epoch seconds.
    pub since: i64,
    /// Whether it was restored from a previous process.
    pub boot: bool,
}

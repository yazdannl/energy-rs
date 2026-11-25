//! Frozen M6 thresholds and exact human-readable texts.

/// Schema version at which a ledger must already carry an audited latch token.
pub const WRITE_AUTHORITY_SCHEMA: u32 = 2;
/// Bounded operator clearance window, milliseconds (M5 owns the timing).
pub const CLEARANCE_WINDOW_MS: i64 = 120_000;
/// Runtime-only advisory session bound, milliseconds.
pub const PROTECTION_ACTIVE_SESSION_MS: i64 = 900_000;
/// Universal startup banner: continuity is never claimed.
pub const CONTINUITY_BANNER: &str =
    "Engine restarted - state continuity not provable; verify mode and protection";
/// Loud write-free text for the latched state.
pub const LATCHED_TEXT: &str =
    "Battery unmanaged — protection writes are disabled. The battery is not being protected.";
/// Loud neutralisation-only text for the clearing state.
pub const CLEARING_TEXT: &str =
    "Clearance in progress — neutralisation only. Full protection has not resumed.";
/// Loud recovery text; no operational state may hide missing authority.
pub const RECOVERY_TEXT: &str = "Authority recovery required — protection writes are disabled. An operator must restore or reinitialise the latch token.";
/// Protection-active text used by both dashboard and API health.
pub const ACTIVE_TEXT: &str = "Battery protection active.";

//! Deterministic fakes for M4 dispatch tests; no live I/O.
#![allow(dead_code)] // Shared across several integration-test binaries.

mod behavior;
mod clock;
mod fake_io;

pub use behavior::Behavior;
pub use clock::ImmediateClock;
pub use fake_io::FakeIo;

use energy_decide::dispatch::FlowReport;

/// Build a fresh flow report with explicit status and fault context.
pub fn report(
    pack: &str,
    at_ms: i64,
    measured_w: f64,
    ac: i32,
    dc: i32,
    message_id: u32,
) -> FlowReport {
    FlowReport {
        pack_id: pack.to_owned(),
        at_ms,
        measured_w,
        ac_status: ac,
        dc_status: dc,
        fault_level: 0,
        soc_pct: 60.0,
        device_min_soc_pct: 10.0,
        echo: None,
        message_id,
    }
}

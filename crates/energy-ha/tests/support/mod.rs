//! Shared test support: a fake Home Assistant WebSocket server and fixtures.
//!
//! Named `support` rather than `common` on purpose (scripts/check_structure.py forbids
//! catch-all module names); the pieces are split by type -- `reply` for what the handler returns,
//! `fake_server` for the server itself.

#![allow(dead_code)]
// Test support: a failed assertion is the report, so the panic-family lints do not apply here.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::float_cmp,
    clippy::manual_midpoint
)]

mod fake_server;
mod reply;

// Each integration test binary includes this module; not every binary uses both types.
#[allow(unused_imports)]
pub use fake_server::FakeServer;
#[allow(unused_imports)]
pub use reply::Reply;

use std::sync::Arc;

use serde_json::Value;

/// The access token the fake server accepts.
pub const TOKEN: &str = "test-token";

/// What the fake server does with one request: `(id, command type, request body) -> Reply`.
pub type Handler = Arc<dyn Fn(u64, &str, &Value) -> Reply + Send + Sync>;

/// Load a scrubbed JSON fixture from `tests/fixtures`.
pub fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    let text = std::fs::read_to_string(&path).expect("read fixture");
    serde_json::from_str(&text).expect("parse fixture")
}

/// The `result` member of a fixture.
pub fn result(name: &str) -> Value {
    fixture(name)
        .get("result")
        .cloned()
        .expect("fixture has result")
}

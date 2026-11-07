//! Engine ports for the integration tests: scripted fleet, meter, sink, plan source and ledger.
//!
//! One public type per file (the structure gate applies to tests too), re-exported here so a test
//! only ever imports `support::fakes::{...}`.

#![allow(unused_imports, dead_code)]

mod fake_fleet;
mod fake_meter;
mod fake_sink;
mod fake_source;
mod fixtures;
mod sent;

pub use fake_fleet::FakeFleet;
pub use fake_meter::FakeMeter;
pub use fake_sink::FakeSink;
pub use fake_source::FakeSource;
pub use fixtures::{NOW, ledger_with, pack, plan};
pub use sent::Sent;

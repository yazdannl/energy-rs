//! The SQLite handle: one mutex-guarded connection, pragmas, and forward-only migrations.
//!
//! Repositories returned by [`Ledger`](crate::db::Ledger) borrow that connection guard, so every
//! read and write in a scope is serialized without handing `rusqlite` types to the rest of the
//! system.

pub mod config;
pub mod ledger;
pub mod migrate;
pub mod migration;
mod migrations;

pub use config::LedgerConfig;
pub use ledger::Ledger;

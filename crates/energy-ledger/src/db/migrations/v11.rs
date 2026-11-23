//! v11: durable outbox for learning audit events.
//!
//! A learning profile can be committed before its `learning.applied` journal write. Without an
//! outbox that audit line is lost forever: the next pass sees the fresh profile and skips the pack.
//! The pending row is written in the same transaction as the profile, and removed only together
//! with a successful event insert.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 11,
    name: "learning-event-outbox",
    sql: "CREATE TABLE learning_outbox (
              id      INTEGER PRIMARY KEY AUTOINCREMENT,
              pack_id TEXT    NOT NULL,
              at      INTEGER NOT NULL,
              summary TEXT    NOT NULL
          );",
};

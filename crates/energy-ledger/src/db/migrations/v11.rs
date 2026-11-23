//! v11: durable outbox for learning audit events, plus its poison quarantine.
//!
//! A learning profile can be committed before its `learning.applied` journal write. Without an
//! outbox that audit line is lost forever: the next pass sees the fresh profile and skips the pack.
//! The pending row is written in the same transaction as the profile, claimed by the publisher, and
//! moved to the quarantine table with its payload if that one row cannot be journalled.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 11,
    name: "learning-event-outbox",
    sql: "CREATE TABLE learning_outbox (
              id      INTEGER PRIMARY KEY AUTOINCREMENT,
              pack_id TEXT    NOT NULL,
              at      INTEGER NOT NULL,
              summary TEXT    NOT NULL
          );
          CREATE TABLE learning_outbox_quarantine (
              id             INTEGER PRIMARY KEY AUTOINCREMENT,
              source_id      INTEGER NOT NULL UNIQUE,
              pack_id        TEXT    NOT NULL,
              at             INTEGER NOT NULL,
              summary        TEXT    NOT NULL,
              raw_pack_id    TEXT    NOT NULL,
              raw_at         TEXT    NOT NULL,
              raw_summary    TEXT    NOT NULL,
              reason         TEXT    NOT NULL,
              quarantined_at INTEGER NOT NULL
          );",
};

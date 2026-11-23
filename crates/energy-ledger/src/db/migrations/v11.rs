//! v11: durable outbox for learning audit events, plus its lossless poison quarantine.
//!
//! A learning profile can be committed before its `learning.applied` journal write. Without an
//! outbox that audit line is lost forever: the next pass sees the fresh profile and skips the pack.
//! The pending row is written in the same transaction as the profile, claimed by the publisher, and
//! moved to the quarantine table with its original SQLite storage types and bytes if that one row
//! cannot be journalled.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 11,
    name: "learning-event-outbox",
    sql: "CREATE TABLE learning_outbox (
              id      INTEGER PRIMARY KEY AUTOINCREMENT,
              pack_id TEXT,
              at      INTEGER,
              summary TEXT
          );
          CREATE TABLE learning_outbox_quarantine (
              id             INTEGER PRIMARY KEY AUTOINCREMENT,
              source_id      INTEGER NOT NULL UNIQUE,
              pack_id_type   TEXT    NOT NULL,
              pack_id_blob   BLOB    NOT NULL,
              at_type        TEXT    NOT NULL,
              at_blob        BLOB    NOT NULL,
              summary_type   TEXT    NOT NULL,
              summary_blob   BLOB    NOT NULL,
              reason         TEXT    NOT NULL,
              quarantined_at INTEGER NOT NULL
          );",
};

//! v13: a release marker for genuinely measured dispatch watts.
//!
//! v10 set `flow_measured = verified` for historical rows, and an echo is not a sample. Legacy
//! rows therefore keep their original value but carry no marker; hydration must treat them as
//! measurement-unknown. New writers set `applied_w_measured` only when a timestamped sample exists.
//! No row is retro-promoted.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 13,
    name: "dispatch-measured-release-marker",
    sql: "ALTER TABLE dispatch_log ADD COLUMN applied_w_measured INTEGER;
          CREATE TRIGGER dispatch_measured_requires_no_reason_insert
            BEFORE INSERT ON dispatch_log
            WHEN NEW.applied_w_measured = 1 AND NEW.applied_w_reason IS NOT NULL
            BEGIN SELECT RAISE(ABORT, 'measured watts cannot have an unknown-watts reason'); END;
          CREATE TRIGGER dispatch_measured_requires_no_reason_update
            BEFORE UPDATE ON dispatch_log
            WHEN NEW.applied_w_measured = 1 AND NEW.applied_w_reason IS NOT NULL
            BEGIN SELECT RAISE(ABORT, 'measured watts cannot have an unknown-watts reason'); END;",
};

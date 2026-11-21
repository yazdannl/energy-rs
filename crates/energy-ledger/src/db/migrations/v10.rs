//! v10: provenance for dispatch power.
//!
//! `applied_w` used to read 0 on a suppressed tick even when the pack was still delivering. Store
//! whether a command reached the device and whether the watts are a fresh measurement, so a
//! `fault_backoff` row can say both "nothing was sent" and "the pack is still flowing".

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 10,
    name: "dispatch-measurement-provenance",
    sql: "ALTER TABLE dispatch_log ADD COLUMN written INTEGER;
          ALTER TABLE dispatch_log ADD COLUMN flow_measured INTEGER NOT NULL DEFAULT 0;
          UPDATE dispatch_log SET flow_measured = verified;",
};

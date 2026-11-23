//! v11: name why a dispatch row has no measured watts.
//!
//! `applied_w` stays 0 for legacy rows and for rows with no measurement, but `flow_measured`
//! alone cannot say whether the sample was stale or the read failed. Store that reason so the API
//! can return `appliedW: null` with an honest machine-readable cause.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 11,
    name: "dispatch-applied-power-reason",
    sql: "ALTER TABLE dispatch_log ADD COLUMN applied_w_reason TEXT;",
};

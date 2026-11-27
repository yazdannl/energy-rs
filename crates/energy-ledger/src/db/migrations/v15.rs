//! v15: the operator's per-pack AC round trip, entered before learning has a confident value.
//!
//! The migration also repairs the provenance stamp on older rows. Before this version, a
//! capacity-only pass could write a *current* `packs.learned_at` while `learned_round_trip` was
//! still the typed seed; the shared stamp is not evidence that a round trip was measured. The
//! durable source of truth is the per-pack learning profile JSON in `settings` under
//! `learning.profile.<serial>`: only `efficiency_applied = true` proves a published measurement.
//!
//! Exact rule, applied once by the UPDATE below: set `learned_at` to the never-measured sentinel
//! (UNIX_EPOCH, 0) unless a readable profile for that serial says `efficiency_applied = true`.
//! Missing, corrupt or explicitly unmeasured profiles therefore become unmeasured rows; a
//! genuinely measured row keeps its stamp and its learned round trip wins over a manual entry.
//! `manual_round_trip` is added as NULL for every pack, so no operator entry is invented.

use crate::db::migration::Migration;

pub(crate) const MIGRATION: Migration = Migration {
    version: 15,
    name: "pack-manual-round-trip",
    sql: "
ALTER TABLE packs ADD COLUMN manual_round_trip REAL;
UPDATE packs SET learned_at = 0
WHERE NOT EXISTS (
    SELECT 1 FROM settings
    WHERE key = 'learning.profile.' || packs.sn
      AND json_valid(value)
      AND json_extract(value, '$.efficiency_applied') = 1
);
",
};

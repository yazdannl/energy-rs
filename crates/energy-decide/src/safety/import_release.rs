//! Audited manual release of a fail-closed import guard.

/// One explicit manual release, retained for the M4 row and the ledger.
#[derive(Debug, Clone, Default, PartialEq)]
#[allow(clippy::struct_field_names)] // engineering units are part of the audit contract
pub struct ImportReleaseRecord {
    pub pack_id: String,
    pub actor: String,
    pub reason: String,
    pub at_ms: i64,
    /// Identity of the last evaluated snapshot used for the release decision.
    pub snapshot_at_ms: i64,
    pub grid_sample_at_ms: Option<i64>,
    pub proposed_plan_w: f64,
    pub projected_import_w: Option<f64>,
    pub current_import_w: Option<f64>,
    pub import_ceiling_w: f64,
    pub import_deadband_w: f64,
    pub release_threshold_w: f64,
    pub settled_grid_w: Option<f64>,
    pub settled_at_ms: Option<i64>,
    pub proposed_plan_breach: bool,
    pub current_import_breach: bool,
    pub breach_remedied: bool,
}

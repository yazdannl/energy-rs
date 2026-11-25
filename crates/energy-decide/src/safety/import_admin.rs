//! Proof-only recovery and guarded manual release of the import latch.

use crate::safety::RecoveryStatus;
use crate::safety::import_release::ImportReleaseRecord;
use crate::safety::ladder::SafetyLadder;
use crate::safety::rules::projected_import;
use crate::safety::rules_import::{
    breach_remedied, current_import_breach, import_guard_active, live_import_breach,
    proposed_plan_breach, release_threshold_w,
};
use crate::safety::state::note;

impl SafetyLadder {
    /// Clear terminal hold proof only; import protection is deliberately left intact.
    pub fn recover_hold(&self, pack_id: &str, _at_ms: i64) -> RecoveryStatus {
        let mut state = self.lock();
        let hold_proof_cleared = state.holds.remove(pack_id).is_some();
        let import_protection_active = import_guard_active(&state, pack_id);
        if hold_proof_cleared {
            note(&mut state, pack_id, "hold_recovered");
            if import_protection_active {
                note(&mut state, pack_id, "import_protection_still_active");
            }
        }
        RecoveryStatus {
            hold_proof_cleared,
            import_protection_active,
        }
    }

    /// Whether import protection is currently armed for one pack.
    pub fn import_protection_active(&self, pack_id: &str) -> bool {
        import_guard_active(&self.lock(), pack_id)
    }

    /// Audited manual releases, in order, for the M4 row and the ledger wiring.
    pub fn import_release_history(&self) -> Vec<ImportReleaseRecord> {
        self.lock().import_releases.clone()
    }

    /// Release an armed import guard with an explicit actor and reason.
    /// A currently evaluated breach is never bypassable by this path.
    pub fn release_import(
        &self,
        pack_id: &str,
        actor: &str,
        reason: &str,
        at_ms: i64,
    ) -> Result<ImportReleaseRecord, &'static str> {
        let mut state = self.lock();
        let input = state
            .inputs
            .get(pack_id)
            .ok_or("import_pack_unknown")?
            .clone();
        if actor.trim().is_empty() || reason.trim().is_empty() {
            return Err("manual_release_audit_required");
        }
        if live_import_breach(&input) {
            return Err("import_breach_active");
        }
        if !state.imports.get(pack_id).is_some_and(|latch| latch.armed) {
            return Err("import_guard_not_armed");
        }
        let record = ImportReleaseRecord {
            pack_id: pack_id.to_owned(),
            actor: actor.to_owned(),
            reason: reason.to_owned(),
            at_ms,
            snapshot_at_ms: input.at_ms,
            grid_sample_at_ms: input.grid_sample_at_ms,
            proposed_plan_w: input.plan_w,
            projected_import_w: projected_import(&input, input.plan_w),
            current_import_w: projected_import(&input, 0.0),
            import_ceiling_w: input.import_ceiling_w,
            import_deadband_w: input.import_deadband_w,
            release_threshold_w: release_threshold_w(&input),
            settled_grid_w: input.settled_grid_w,
            settled_at_ms: input.settled_at_ms,
            proposed_plan_breach: proposed_plan_breach(&input),
            current_import_breach: current_import_breach(&input),
            breach_remedied: breach_remedied(&input, &state, at_ms),
        };
        if let Some(latch) = state.imports.get_mut(pack_id) {
            latch.armed = false;
            latch.ask_w = 0.0;
            latch.unavailable_reason = None;
        }
        state.import_releases.push(record.clone());
        note(&mut state, pack_id, "import_guard_manually_released");
        Ok(record)
    }
}

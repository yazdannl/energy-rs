//! Proof-only recovery and guarded manual release of the import latch.

use crate::safety::RecoveryStatus;
use crate::safety::import_release::ImportReleaseRecord;
use crate::safety::ladder::SafetyLadder;
use crate::safety::rules_import::{import_breach_active, import_guard_active};
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
        if import_breach_active(&input) {
            return Err("import_breach_active");
        }
        let latch = state
            .imports
            .get_mut(pack_id)
            .ok_or("import_guard_not_armed")?;
        if !latch.armed {
            return Err("import_guard_not_armed");
        }
        latch.armed = false;
        latch.ask_w = 0.0;
        latch.unavailable_reason = None;
        let record = ImportReleaseRecord {
            pack_id: pack_id.to_owned(),
            actor: actor.to_owned(),
            reason: reason.to_owned(),
            at_ms,
        };
        state.import_releases.push(record.clone());
        note(&mut state, pack_id, "import_guard_manually_released");
        Ok(record)
    }
}

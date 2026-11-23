//! Map one stored ledger session into the learner's plain record.

use energy_battery::control::Direction as BatteryDirection;
use energy_battery::learning::SessionRecord;
use energy_ledger::Session;
use energy_ledger::session::Direction as LedgerDirection;

use crate::engine::instant;

/// Keep invalid runs as records with their direction, time bounds and refusal marker.
///
/// Dropping them upstream would erase a direction change: a discarded discharge between two charges
/// would let the learner merge the charges and book stored energy as a loss.
pub(super) fn map(session: &Session, pack_sn: &str) -> SessionRecord {
    SessionRecord {
        pack_id: pack_sn.to_owned(),
        direction: match session.direction {
            LedgerDirection::Charge => BatteryDirection::Charge,
            LedgerDirection::Discharge => BatteryDirection::Discharge,
        },
        started_at: instant(session.started_at),
        ended_at: instant(session.ended_at),
        soc_start_pct: session.soc_start.unwrap_or(f64::NAN),
        soc_end_pct: session.soc_end.unwrap_or(f64::NAN),
        energy_ac_kwh: session.energy_ac_kwh,
        duration_s: session.duration_s as f64,
        sample_count: u32::try_from(session.sample_count).unwrap_or(u32::MAX),
        complete: session.complete && session.invalidated_reason.is_none(),
    }
}

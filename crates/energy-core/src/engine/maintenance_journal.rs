//! Journal events for the price-aware balancing rule.

use serde_json::json;

use energy_battery::protection::full_charge::FullChargeAssignment;
use energy_ledger::EventLevel;

use crate::engine::control::Engine;
use crate::engine::error::EngineError;
use crate::maintenance::DwellOutcome;

impl Engine {
    /// Append one journal event with a machine-readable payload.
    pub(crate) fn event_data(
        &self,
        at: i64,
        level: EventLevel,
        kind: &str,
        message: impl Into<String>,
        data: &serde_json::Value,
    ) -> Result<(), EngineError> {
        let event = energy_ledger::Event::new(at, level, kind, message).with_data(data.to_string());
        self.ledger.events()?.insert(&event)?;
        Ok(())
    }

    /// One `maintenance.planned` event per due pack, with slots and the price expectation.
    pub(crate) fn journal_maintenance(
        &self,
        at: i64,
        assignments: &[FullChargeAssignment],
    ) -> Result<(), EngineError> {
        for assignment in assignments {
            let slots: Vec<serde_json::Value> = assignment
                .slots
                .iter()
                .map(|slot| {
                    json!({
                        "tStart": slot.t_start,
                        "priceEurPerKwh": slot.price_eur_per_kwh,
                        "chargeW": slot.charge_w,
                    })
                })
                .collect();
            self.event_data(
                at,
                EventLevel::Info,
                "maintenance.planned",
                format!(
                    "{}: {} full charge to 100 % in {} cheap slots, expected {:.3} EUR",
                    assignment.pack_id,
                    assignment.reason,
                    assignment.slots.len(),
                    assignment.expected_cost_eur
                ),
                &json!({
                    "pack": assignment.pack_id,
                    "reason": assignment.reason,
                    "targetSocPct": assignment.target_soc_pct,
                    "slots": slots,
                    "expectedCostEur": assignment.expected_cost_eur,
                    "dwellUntil": assignment.dwell_until,
                }),
            )?;
        }
        Ok(())
    }

    /// One event for each completed or abandoned balancing dwell.
    pub(crate) fn journal_dwell(&self, at: i64, outcome: &DwellOutcome) -> Result<(), EngineError> {
        if !outcome.completed {
            return self.event_data(
                at,
                EventLevel::Warn,
                "maintenance.dwell_failed",
                format!(
                    "{}: balancing dwell abandoned ({})",
                    outcome.pack_id, outcome.reason
                ),
                &json!({"pack": outcome.pack_id, "reason": outcome.reason}),
            );
        }
        self.event_data(
            at,
            EventLevel::Info,
            "maintenance.completed",
            format!(
                "{}: balancing dwell completed; the seven-day clock starts now",
                outcome.pack_id
            ),
            &json!({"pack": outcome.pack_id, "lastFullAt": at}),
        )
    }
}

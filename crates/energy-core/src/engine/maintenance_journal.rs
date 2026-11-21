//! Journal events for the price-aware balancing rule.

use serde_json::json;

use energy_battery::protection::full_charge::{FullChargeAssignment, UnassignedPack};
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
        unassigned: &[UnassignedPack],
    ) -> Result<(), EngineError> {
        for assignment in assignments {
            self.journal_assignment(at, assignment)?;
        }
        for pack in unassigned {
            self.event_data(
                at,
                EventLevel::Warn,
                "maintenance.unassigned",
                format!(
                    "{}: no cheap window before {}: {:.3} kWh missing ({})",
                    pack.pack_id, pack.deadline, pack.missing_kwh, pack.reason
                ),
                &json!({
                    "pack": pack.pack_id,
                    "reason": pack.reason,
                    "requiredKwh": pack.required_kwh,
                    "availableKwh": pack.available_kwh,
                    "missingKwh": pack.missing_kwh,
                    "deadline": pack.deadline,
                }),
            )?;
        }
        Ok(())
    }

    /// One `maintenance.planned` event, marked partial when the band could not finish the pack.
    fn journal_assignment(
        &self,
        at: i64,
        assignment: &FullChargeAssignment,
    ) -> Result<(), EngineError> {
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
        let message = if assignment.is_complete() {
            format!(
                "{}: {} full charge to 100 % in {} cheap slots, expected {:.3} EUR",
                assignment.pack_id,
                assignment.reason,
                assignment.slots.len(),
                assignment.expected_cost_eur
            )
        } else {
            format!(
                "{}: {} partial charge in {} cheap slots, {:.3} kWh still missing ({})",
                assignment.pack_id,
                assignment.reason,
                assignment.slots.len(),
                assignment.missing_kwh,
                assignment.partial_reason.unwrap_or("partial")
            )
        };
        self.event_data(
            at,
            EventLevel::Info,
            "maintenance.planned",
            message,
            &json!({
                "pack": assignment.pack_id,
                "reason": assignment.reason,
                "targetSocPct": assignment.target_soc_pct,
                "slots": slots,
                "expectedCostEur": assignment.expected_cost_eur,
                "dwellUntil": assignment.dwell_until,
                "missingKwh": assignment.missing_kwh,
                "partialReason": assignment.partial_reason,
            }),
        )
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

//! Delivery classification from requested pack powers and fresh measurements.

use crate::plan_assemble::{DeliveryOutcome, PackDelivery, PackPower};

#[derive(Debug, Clone, PartialEq)]
pub struct Delivery {
    /// Full, partial or failed delivery.
    pub outcome: DeliveryOutcome,
    /// Packs that did not deliver.
    pub failed_pack_ids: Vec<String>,
    /// Sum of measured powers, W.
    pub applied_w: f64,
    /// Sum of requested powers minus applied powers, W.
    pub shortfall_w: f64,
    /// Per-pack evidence rows.
    pub rows: Vec<PackDelivery>,
}

impl Default for Delivery {
    fn default() -> Self {
        Self {
            outcome: DeliveryOutcome::Full,
            failed_pack_ids: Vec::new(),
            applied_w: 0.0,
            shortfall_w: 0.0,
            rows: Vec::new(),
        }
    }
}

/// Classify delivery without executing or writing anything.
pub fn classify_delivery(planned: &[PackPower], measured: &[PackPower]) -> Delivery {
    let mut delivery = Delivery::default();
    for request in planned {
        let found = measured.iter().find(|row| row.pack_id == request.pack_id);
        let applied = found.map_or(0.0, |row| row.w);
        let missing = found.is_none();
        delivery.rows.push(PackDelivery {
            pack_id: request.pack_id.clone(),
            planned_w: request.w,
            applied_w: applied,
            reason: if missing { "orphan_hold" } else { "measured" },
            written: if missing { "attempted" } else { "verified" },
        });
        delivery.applied_w += applied;
        delivery.shortfall_w += (request.w - applied).abs();
        if missing {
            delivery.failed_pack_ids.push(request.pack_id.clone());
        }
    }
    delivery.outcome = if delivery.failed_pack_ids.is_empty() {
        DeliveryOutcome::Full
    } else if delivery.failed_pack_ids.len() == planned.len() {
        DeliveryOutcome::Failed
    } else {
        DeliveryOutcome::Partial
    };
    delivery
}

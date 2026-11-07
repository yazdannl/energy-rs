//! Measured savings: catch up on boot, then keep yesterday and today current.
//!
//! The rollup is a background loop, not part of the control tick: a savings day is derived from
//! measured statistics that only exist after the fact, and a slow Home Assistant query must never
//! delay a battery decision. The first iteration runs immediately, so a restart after downtime
//! recomputes the missed days before the first quarter-hour passes.

use std::sync::Arc;
use std::time::Duration;

use energy_core::savings::{HaSavingsSource, SavingsRollup};
use tokio::time::MissedTickBehavior;

use crate::bootstrap::ha::Ha;
use crate::bootstrap::prices::Prices;
use crate::service::Service;

/// Recompute cadence: once per quarter-hour, just after a slot closed.
const REFRESH: Duration = Duration::from_mins(15);

/// Start the savings rollup loop.
pub fn spawn(ha: &Ha, prices: &Arc<Prices>, service: &Arc<Service>) -> tokio::task::JoinHandle<()> {
    let source = Arc::new(HaSavingsSource::new(
        ha.client(),
        Arc::clone(&prices.repository),
    ));
    let rollup = SavingsRollup::new(Arc::clone(service.ledger()), source, prices.settlement());
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(REFRESH);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let now = chrono::Utc::now().timestamp();
            match rollup.refresh(now).await {
                Ok(report) if !report.failures.is_empty() => {
                    tracing::warn!(failures = ?report.failures, "some savings days could not be read");
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(%error, "savings rollup failed; the next quarter retries");
                }
            }
        }
    })
}

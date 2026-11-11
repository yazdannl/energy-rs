//! The stored-energy cost pool (`docs/POLICY.md` §5).
//!
//! `b` is the average price paid for the energy that is in the pack: grid kWh enter at the buy
//! price, PV kWh at the feed-in they forgo, and a discharge relieves cost at the running average.
//! It is a stock, not a window: a rolling average would let cheap recent charging erase an
//! expensive pack's basis.

use energy_ledger::pool::PoolRow;

use crate::savings::SlotFlowAc;

/// Advance the pool with one measured slot.
///
/// `pv_ac` enters at the forgone feed-in; a discharge relieves cost at the running average. A slot
/// whose flows are unknown must not call this at all (no zero-filling).
pub fn advance(pool: PoolRow, at: i64, flow: SlotFlowAc) -> PoolRow {
    let SlotFlowAc {
        charge_ac,
        pv_ac,
        discharge_ac,
        buy,
        feed_in,
        eta_charge,
        eta_discharge,
    } = flow;
    // AC in, stored out: the pool only ever holds stored energy.
    let eta_in = if eta_charge.is_finite() {
        eta_charge.clamp(0.05, 1.0)
    } else {
        1.0
    };
    let eta_out = if eta_discharge.is_finite() {
        eta_discharge.clamp(0.05, 1.0)
    } else {
        1.0
    };
    let charge = charge_ac.max(0.0);
    let pv = pv_ac.clamp(0.0, charge);
    let grid = charge - pv;
    // What it cost is what was paid at the meter, whatever survives the conversion.
    let inflow = if grid.is_finite() && buy.is_finite() {
        grid * buy
    } else {
        0.0
    } + if pv.is_finite() && feed_in.is_finite() {
        pv * feed_in.max(-1.0)
    } else {
        0.0
    };
    let stored_in = (grid + pv) * eta_in;
    let relief = if pool.kwh > 0.0 {
        pool.cost_eur / pool.kwh
    } else {
        0.0
    };
    // AC out removes stored energy at the discharge efficiency.
    let out = (discharge_ac.max(0.0) / eta_out).min(pool.kwh.max(0.0));
    let kwh = (pool.kwh + stored_in - out).max(0.0);
    let cost_eur = (pool.cost_eur + inflow - out * relief).max(0.0);
    PoolRow {
        kwh,
        cost_eur: cost_eur.min(kwh * 10.0),
        updated_at: at,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{PoolRow, SlotFlowAc, advance};

    fn flow(charge: f64, pv: f64, buy: f64, feed_in: f64, discharge: f64) -> SlotFlowAc {
        SlotFlowAc {
            charge_ac: charge,
            pv_ac: pv,
            discharge_ac: discharge,
            buy,
            feed_in,
            eta_charge: 1.0,
            eta_discharge: 1.0,
        }
    }

    fn pool(kwh: f64, cost: f64) -> PoolRow {
        PoolRow {
            kwh,
            cost_eur: cost,
            updated_at: 0,
        }
    }

    #[test]
    fn a_grid_charge_moves_the_basis_to_its_price() {
        let after = advance(pool(0.0, 0.0), 1, flow(2.0, 0.0, 0.30, 0.05, 0.0));
        assert!((after.basis().unwrap() - 0.30).abs() < 1e-9);
    }

    #[test]
    fn pv_enters_at_the_forgone_feed_in() {
        let after = advance(pool(0.0, 0.0), 1, flow(2.0, 2.0, 0.30, 0.06, 0.0));
        assert!((after.basis().unwrap() - 0.06).abs() < 1e-9);
    }

    #[test]
    fn a_discharge_relieves_at_the_average_not_the_newest_price() {
        let first = advance(pool(0.0, 0.0), 1, flow(2.0, 0.0, 0.30, 0.05, 0.0));
        let second = advance(first, 2, flow(2.0, 0.0, 0.10, 0.05, 0.0));
        assert!((second.basis().unwrap() - 0.20).abs() < 1e-9);
        let drained = advance(second, 3, flow(0.0, 0.0, 0.0, 0.0, 4.0));
        assert!(drained.kwh.abs() < 1e-9);
        assert!(drained.cost_eur.abs() < 1e-6);
    }

    #[test]
    fn an_empty_pool_has_no_basis() {
        assert!(pool(0.0, 0.0).basis().is_none());
    }
}

#[cfg(test)]
mod efficiency {
    use super::{PoolRow, SlotFlowAc, advance};

    fn flow(charge: f64, discharge: f64, eta: f64) -> SlotFlowAc {
        SlotFlowAc {
            charge_ac: charge,
            pv_ac: 0.0,
            discharge_ac: discharge,
            buy: 0.20,
            feed_in: 0.05,
            eta_charge: eta,
            eta_discharge: eta,
        }
    }

    #[test]
    fn a_round_trip_does_not_create_energy() {
        let mut pool = PoolRow::default();
        // 1 kWh AC in at 95 %, then 1 kWh AC out at 95 %: 0.9025 in, 1.0526 out -- the pool empties
        // and the basis rises, exactly as the pack does.
        pool = advance(pool, 1, flow(1.0, 0.0, 0.95));
        assert!((pool.kwh - 0.95).abs() < 1e-9, "{}", pool.kwh);
        assert!((pool.cost_eur - 0.20).abs() < 1e-9, "{}", pool.cost_eur);
        let basis_before = pool.cost_eur / pool.kwh;
        assert!((basis_before - 0.20 / 0.95).abs() < 1e-9);

        pool = advance(pool, 2, flow(0.0, 0.95, 0.95));
        assert!(
            pool.kwh.abs() < 1e-9,
            "the pack is empty, not negative: {}",
            pool.kwh
        );
        assert!(
            pool.cost_eur.abs() < 1e-9,
            "and it owes nothing: {}",
            pool.cost_eur
        );
    }

    #[test]
    fn the_cost_is_what_was_paid_not_what_survived_the_loss() {
        // 1 kWh AC at 0.20 with a 90 % charge: the basis is 0.20 per stored kWh, not 0.18.
        let pool = advance(PoolRow::default(), 1, flow(1.0, 0.0, 0.90));
        let basis = pool.cost_eur / pool.kwh;
        assert!((basis - 0.20 / 0.90).abs() < 1e-9, "{basis}");
    }
}

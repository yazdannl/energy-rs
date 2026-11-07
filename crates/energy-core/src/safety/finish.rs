//! Combine the bounds into one decision and name every binding reason.

use super::EPS_W;
use super::Reason;
use super::decision::SafetyDecision;
use super::ladder::{Ladder, push_if};

impl Ladder<'_> {
    pub(crate) fn finish(self) -> SafetyDecision {
        let lo = self.lo();
        let hi = self.hi();
        if self.grid_missing {
            return SafetyDecision::blocked(
                self.request.requested_w,
                lo,
                hi,
                vec![Reason::GridTelemetryMissing],
            );
        }
        if lo > hi {
            let reasons = self.block_reasons(lo, hi);
            return SafetyDecision::blocked(self.request.requested_w, lo, hi, reasons);
        }
        let allowed = self.target.clamp(lo, hi);
        let reasons = self.applied_reasons(allowed, lo, hi);
        SafetyDecision::allowed(self.request.requested_w, allowed, lo, hi, reasons)
    }

    fn lo(&self) -> f64 {
        let mut lo = self
            .soc_lo
            .max(self.p_min)
            .max(self.grid_lo)
            .max(self.ramp_lo);
        for bound in [self.track_lo, self.floor_lo, self.latch_lo]
            .into_iter()
            .flatten()
        {
            lo = lo.max(bound);
        }
        lo
    }

    fn hi(&self) -> f64 {
        self.soc_hi
            .min(self.p_max)
            .min(self.grid_hi)
            .min(self.ramp_hi)
            .min(self.track_hi.unwrap_or(f64::INFINITY))
    }

    fn block_reasons(&self, lo: f64, hi: f64) -> Vec<Reason> {
        let mut reasons = Vec::new();
        push_if(&mut reasons, self.soc_hi < lo - EPS_W, Reason::SocCeiling);
        push_if(&mut reasons, self.soc_lo > hi + EPS_W, Reason::SocFloor);
        push_if(
            &mut reasons,
            self.track_hi.is_some_and(|v| v < lo - EPS_W)
                || self.track_lo.is_some_and(|v| v > hi + EPS_W),
            Reason::PlanTarget,
        );
        push_if(
            &mut reasons,
            self.grid_export && self.grid_lo > hi + EPS_W,
            Reason::GridExportLimit,
        );
        push_if(
            &mut reasons,
            self.grid_import && self.grid_hi < lo - EPS_W,
            Reason::GridImportLimit,
        );
        push_if(
            &mut reasons,
            self.ramp_hi < lo - EPS_W || self.ramp_lo > hi + EPS_W,
            Reason::RampLimit,
        );
        push_if(
            &mut reasons,
            self.floor_lo.is_some_and(|v| v > hi + EPS_W),
            Reason::SellFloor,
        );
        push_if(
            &mut reasons,
            self.latch_lo.is_some_and(|v| v > hi + EPS_W),
            Reason::CellVoltageGuard,
        );
        push_if(
            &mut reasons,
            lo > self.p_max + EPS_W || hi < self.p_min - EPS_W,
            Reason::PowerClamp,
        );
        if reasons.is_empty() {
            reasons.push(Reason::PowerClamp);
        }
        reasons
    }

    fn applied_reasons(&self, allowed: f64, lo: f64, hi: f64) -> Vec<Reason> {
        let mut reasons = Vec::new();
        reasons.extend(self.plan_and_soc_reasons(allowed, lo, hi));
        reasons.extend(self.power_and_grid_reasons(allowed, lo, hi));
        reasons.extend(self.tail_reasons(allowed, lo));
        reasons
    }

    fn plan_and_soc_reasons(&self, allowed: f64, lo: f64, hi: f64) -> Vec<Reason> {
        let target = self.target;
        let high = allowed >= hi - EPS_W && target > hi + EPS_W;
        let low = allowed <= lo + EPS_W && target < lo - EPS_W;
        let mut reasons = Vec::new();
        push_if(
            &mut reasons,
            high && self.track_hi.is_some_and(|v| (allowed - v).abs() <= EPS_W),
            Reason::PlanTarget,
        );
        push_if(
            &mut reasons,
            low && self.track_lo.is_some_and(|v| (allowed - v).abs() <= EPS_W),
            Reason::PlanTarget,
        );
        push_if(
            &mut reasons,
            high && (allowed - self.soc_hi).abs() <= EPS_W,
            Reason::SocCeiling,
        );
        push_if(
            &mut reasons,
            low && (allowed - self.soc_lo).abs() <= EPS_W,
            Reason::SocFloor,
        );
        reasons
    }

    fn power_and_grid_reasons(&self, allowed: f64, lo: f64, hi: f64) -> Vec<Reason> {
        let target = self.target;
        let high = allowed >= hi - EPS_W && target > hi + EPS_W;
        let low = allowed <= lo + EPS_W && target < lo - EPS_W;
        let mut reasons = Vec::new();
        push_if(
            &mut reasons,
            high && (allowed - self.p_max).abs() <= EPS_W,
            Reason::PowerClamp,
        );
        push_if(
            &mut reasons,
            low && (allowed - self.p_min).abs() <= EPS_W,
            Reason::PowerClamp,
        );
        push_if(
            &mut reasons,
            high && self.grid_import && self.grid_hi < self.p_max - EPS_W,
            Reason::GridImportLimit,
        );
        push_if(
            &mut reasons,
            low && self.grid_export && self.grid_lo > self.p_min + EPS_W,
            Reason::GridExportLimit,
        );
        reasons
    }

    fn tail_reasons(&self, allowed: f64, lo: f64) -> Vec<Reason> {
        let target = self.target;
        let low = allowed <= lo + EPS_W && target < lo - EPS_W;
        let mut reasons = Vec::new();
        push_if(
            &mut reasons,
            (allowed - self.ramp_hi).abs() <= EPS_W && target > self.ramp_hi + EPS_W,
            Reason::RampLimit,
        );
        push_if(
            &mut reasons,
            (allowed - self.ramp_lo).abs() <= EPS_W && target < self.ramp_lo - EPS_W,
            Reason::RampLimit,
        );
        push_if(
            &mut reasons,
            low && self.floor_lo.is_some_and(|v| (allowed - v).abs() <= EPS_W),
            Reason::SellFloor,
        );
        push_if(
            &mut reasons,
            self.latch_lo.is_some() && self.request.requested_w < -EPS_W,
            Reason::CellVoltageGuard,
        );
        push_if(
            &mut reasons,
            self.forced_charge && allowed > EPS_W,
            Reason::CellVoltageEmergencyCharge,
        );
        reasons
    }
}

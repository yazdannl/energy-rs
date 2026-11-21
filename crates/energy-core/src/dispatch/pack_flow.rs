//! Direction-specific flow evidence from a pack report.

use energy_battery::control::flow::FLOW_THRESHOLD_W;

/// The AC/DC fields that distinguish an echoed limit from power actually moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackFlow {
    /// Grid input: positive while charging from AC.
    pub grid_input_w: i64,
    /// Pack output: positive while charging from AC on a SolarFlow 800 Plus.
    pub output_pack_w: i64,
    /// Home output: positive while discharging to AC.
    pub output_home_w: i64,
    /// `acStatus` (2 = AC input, 1 = AC output) when the firmware reports it.
    pub ac_status: Option<i32>,
    /// `dcStatus` (1 = DC/DC running) when the firmware reports it.
    pub dc_status: Option<i32>,
}

impl PackFlow {
    /// Measured charging power, W; `packInputPower` is PV-to-pack and deliberately excluded.
    #[must_use]
    pub fn charge_w(&self) -> i64 {
        self.grid_input_w.max(self.output_pack_w)
    }

    /// Whether this report proves the commanded direction is flowing at the expected magnitude.
    ///
    /// Charge keeps the hardware-verified status gate. Discharge is power-authoritative: this
    /// fleet has never discharged, so an unverified status value must not stop the import ceiling.
    #[must_use]
    pub fn follows(&self, commanded_w: f64, tolerance_w: f64) -> bool {
        if commanded_w.abs() < f64::EPSILON {
            return self.grid_input_w.abs() as f64 <= tolerance_w
                && self.output_home_w.abs() as f64 <= tolerance_w;
        }
        let measured = if commanded_w.is_sign_negative() {
            self.output_home_w
        } else {
            self.charge_w()
        };
        if measured <= 0 {
            return false;
        }
        if commanded_w > 0.0 && (self.ac_status != Some(2) || self.dc_status != Some(1)) {
            return false;
        }
        let magnitude = commanded_w.abs();
        let threshold = FLOW_THRESHOLD_W as f64;
        // Any real flow in the commanded direction is compliance: a pack that can only manage
        // 45 W of a 1000 W command is current-limited, not ignoring us. The threshold only keeps
        // noise from counting as flow; magnitude mismatch no longer causes a control fault.
        (measured as f64) > threshold
            && ((measured as f64 - magnitude).abs() <= tolerance_w || (measured as f64) < magnitude)
    }

    /// Whether the report proves a failed start rather than legitimate low output.
    #[must_use]
    pub fn failed_start(&self, commanded_w: f64) -> bool {
        if commanded_w.abs() < f64::EPSILON {
            return false;
        }
        let measured = if commanded_w.is_sign_negative() {
            self.output_home_w
        } else {
            self.charge_w()
        };
        measured <= FLOW_THRESHOLD_W
            || (commanded_w > 0.0 && (self.ac_status != Some(2) || self.dc_status != Some(1)))
    }

    /// A status value contradicting sustained discharge power: the mapping is wrong, not the pack.
    #[must_use]
    pub fn discharge_status_alarm(&self) -> Option<String> {
        if self.output_home_w <= FLOW_THRESHOLD_W {
            return None;
        }
        if self.ac_status.is_none_or(|status| status == 1)
            && self.dc_status.is_none_or(|status| status == 1)
        {
            return None;
        }
        Some(format!(
            "discharge is moving {} W to home but acStatus {:?}/dcStatus {:?} disagrees; status mapping needs review",
            self.output_home_w, self.ac_status, self.dc_status
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::PackFlow;

    fn charge_at(grid_w: i64) -> PackFlow {
        PackFlow {
            grid_input_w: grid_w,
            output_pack_w: 0,
            output_home_w: 0,
            ac_status: Some(2),
            dc_status: Some(1),
        }
    }

    fn discharging(home_w: i64) -> PackFlow {
        PackFlow {
            grid_input_w: 0,
            output_pack_w: 0,
            output_home_w: home_w,
            ac_status: Some(2),
            dc_status: Some(0),
        }
    }

    #[test]
    fn discharge_power_is_authoritative_over_an_unverified_status_mapping() {
        let flow = discharging(300);
        assert!(flow.follows(-300.0, 250.0));
        assert!(!flow.failed_start(-300.0));
        assert!(flow.discharge_status_alarm().is_some());
    }

    #[test]
    fn a_low_but_real_taper_is_not_a_failed_start() {
        let flow = PackFlow {
            grid_input_w: 20,
            output_pack_w: 0,
            output_home_w: 0,
            ac_status: Some(2),
            dc_status: Some(1),
        };
        assert!(!flow.failed_start(45.0));
        assert!(flow.follows(45.0, 250.0));
        let tapered = PackFlow {
            grid_input_w: 45,
            ac_status: Some(2),
            dc_status: Some(1),
            ..flow
        };
        assert!(
            tapered.follows(800.0, 250.0),
            "45 W of real flow clears an 800 W command's readback too"
        );
        assert!(discharging(0).failed_start(-300.0));
        let noise = charge_at(5);
        assert!(!noise.follows(500.0, 250.0));
        assert!(noise.failed_start(500.0));
        let tiny = charge_at(6);
        assert!(
            tiny.follows(500.0, 250.0),
            "6 W is above the shared noise floor"
        );
        assert!(!tiny.failed_start(500.0));
    }
}

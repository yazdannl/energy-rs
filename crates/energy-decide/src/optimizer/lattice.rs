//! The configured 0.05 kWh state lattice and its exact anchors.

use crate::optimizer::{OptimizerConfig, OptimizerRefusal};

/// Coarse state lattice used for bound/anchor validation and terminal-domain checks.
///
/// The canonical DP resolves energy to 1 Wh (the oracle resolution); this lattice is the
/// configured `soc_step_kwh` domain from which the DP's floor, ceiling and anchors are taken.
#[derive(Debug, Clone, PartialEq)]
pub struct StateLattice {
    /// Sorted lattice points in kWh: floor..ceiling plus exact start/target anchors.
    pub points_kwh: Vec<f64>,
    /// Step actually used after the `max_states` coarsening, kWh.
    pub step_kwh: f64,
    /// Hard floor, kWh.
    pub floor_kwh: f64,
    /// Hard ceiling, kWh.
    pub ceiling_kwh: f64,
}

impl StateLattice {
    /// Build the lattice, refusing contradictory or non-finite bounds before solving.
    pub fn new(config: &OptimizerConfig) -> Result<Self, OptimizerRefusal> {
        let floor = config.hard_floor_kwh;
        let ceiling = config.ceiling_kwh;
        if !floor.is_finite() || !ceiling.is_finite() {
            return Err(OptimizerRefusal::NonFiniteConfig {
                field: "battery bounds",
            });
        }
        if floor > ceiling {
            return Err(OptimizerRefusal::InvalidBounds {
                floor_kwh: floor,
                ceiling_kwh: ceiling,
            });
        }
        let span = ceiling - floor;
        let mut step = config.soc_step_kwh.max(1e-4);
        let budget = config.max_states.saturating_sub(3).max(1) as f64;
        if span / step > budget - 1.0 {
            step = span / (budget - 1.0).max(1.0);
        }
        if !step.is_finite() || step <= 0.0 {
            return Err(OptimizerRefusal::NonFiniteConfig {
                field: "soc_step_kwh",
            });
        }
        let mut count = 1usize;
        let mut point = floor + step;
        while point <= ceiling && count < config.max_states {
            count += 1;
            point += step;
        }
        let mut points: Vec<f64> = (0..count)
            .map(|index| floor + index as f64 * step)
            .collect();
        points.push(ceiling);
        for anchor in [config.start_kwh, config.effective_target_kwh()] {
            if anchor >= floor && anchor <= ceiling {
                points.push(anchor);
            }
        }
        points.sort_by(f64::total_cmp);
        points.dedup_by(|left, right| (*left - *right).abs() <= 1e-9);
        Ok(Self {
            points_kwh: points,
            step_kwh: step,
            floor_kwh: floor,
            ceiling_kwh: ceiling,
        })
    }

    /// Whether an exact energy sits inside the hard SoC band.
    pub fn contains(&self, energy_kwh: f64) -> bool {
        energy_kwh >= self.floor_kwh - 1e-9 && energy_kwh <= self.ceiling_kwh + 1e-9
    }
}

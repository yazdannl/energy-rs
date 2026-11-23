//! The action grid: 24 evenly spaced powers across the battery window, plus zero.

/// Quantised aggregate AC powers (W), sorted and deduplicated.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionGrid {
    /// Every candidate power, including both endpoints and zero.
    pub powers_w: Vec<i32>,
}

impl ActionGrid {
    /// Build the grid from the aggregate battery window.
    pub fn new(charge_w: f64, discharge_w: f64, steps: usize) -> Self {
        let lo = -discharge_w;
        let hi = charge_w;
        let steps = steps.max(1);
        let mut values = Vec::with_capacity(steps + 1);
        if steps == 1 {
            values.push(f64::midpoint(lo, hi).round() as i32);
        } else {
            for index in 0..steps {
                let fraction = index as f64 / (steps - 1) as f64;
                values.push((lo + (hi - lo) * fraction).round() as i32);
            }
        }
        values.push(0);
        values.sort_unstable();
        values.dedup();
        Self { powers_w: values }
    }
}

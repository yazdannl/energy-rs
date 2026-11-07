//! The physical envelope a learned capacity is published inside.

/// Half-width of the band around the pack label, as a fraction.
///
/// A pack may be older than its label, but an AC-only estimate cannot prove it holds *more*: the
/// two leg efficiencies are not separately identifiable from AC energy and SoC, so anything above
/// the label plus this band is a measurement error, not a bigger pack.
pub const BAND_FRACTION: f64 = 0.25;

/// Where a learned capacity may be published: the label it belongs to and the pack's own figure.
#[derive(Debug, Clone, PartialEq)]
pub struct CapacityBounds {
    /// The pack label from the report's `packType` (`AB2000` = 1.92 kWh), the band's centre.
    pub label_kwh: f64,
    /// The capacity the pack itself reports, when it reports one at all: a hard ceiling.
    pub reported_kwh: Option<f64>,
}

impl CapacityBounds {
    /// Bounds for one pack. A missing or nonsensical label means "no capacity may be published".
    pub fn new(label_kwh: f64, reported_kwh: Option<f64>) -> Self {
        Self {
            label_kwh,
            reported_kwh,
        }
    }

    /// Whether there is a label to publish against at all.
    pub fn is_usable(&self) -> bool {
        self.label_kwh.is_finite() && self.label_kwh > 0.0
    }

    /// The `[low, high]` kWh interval an estimate may be published in.
    ///
    /// `high` is the reported capacity when that is lower than the band's top, so a measurement is
    /// always free to correct *downward* (aging) and never allowed to claim more than the pack's
    /// own figure. A reported capacity below the band's floor collapses the interval onto it: the
    /// ceiling still wins, because it is what the hardware says about itself.
    pub fn band(&self) -> (f64, f64) {
        if !self.is_usable() {
            return (0.0, 0.0);
        }
        let top = self.label_kwh * (1.0 + BAND_FRACTION);
        let ceiling = self
            .reported_kwh
            .filter(|reported| reported.is_finite() && *reported > 0.0)
            .map_or(top, |reported| reported.min(top));
        let floor = self.label_kwh * (1.0 - BAND_FRACTION);
        if floor > ceiling {
            (ceiling, ceiling)
        } else {
            (floor, ceiling)
        }
    }
}

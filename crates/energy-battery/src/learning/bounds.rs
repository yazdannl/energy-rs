//! The physical envelope a learned capacity is published inside.

/// Half-width of the *default* band below the pack label, as a fraction.
///
/// An AC-only estimate can correct an aged pack downward, but the two leg efficiencies are not
/// separately identifiable from AC energy and SoC, so a small downward envelope is all the routine
/// evidence supports. Upward movement is even smaller ([`UPWARD_FRACTION`]): a pack cannot gain
/// cells, and default-efficiency noise tends to read high.
pub const BAND_FRACTION: f64 = 0.10;
/// How far above the label an AC-only estimate may move.
pub const UPWARD_FRACTION: f64 = 0.06;
/// Extra downward room opened only by long-span, high-information evidence of real aging.
pub const AGING_FRACTION: f64 = 0.16;
/// Information confidence needed before the aging envelope may be used.
pub const AGING_CONFIDENCE: f64 = 0.80;

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

    /// The `[low, high]` kWh interval an AC-only estimate may be published in.
    ///
    /// `high` is the reported capacity when that is lower than the band's top, so a measurement is
    /// always free to correct *downward* (aging) and never allowed to claim more than the pack's own
    /// figure. A reported capacity below the band's floor collapses the interval onto it: the
    /// ceiling still wins, because it is what the hardware says about itself.
    pub fn band(&self) -> (f64, f64) {
        self.band_at(self.label_kwh * (1.0 - BAND_FRACTION))
    }

    /// The wider downward interval reserved for strong evidence of genuine aging.
    pub fn aging_band(&self) -> (f64, f64) {
        self.band_at(self.label_kwh * (1.0 - AGING_FRACTION))
    }

    /// The figure an AC-only estimate starts from: the pack's own report when it has one, else the
    /// label. Both are clamped to the ordinary band so a corrupt report cannot become a prior.
    pub fn prior_kwh(&self) -> f64 {
        let (low, high) = self.band();
        self.reported_kwh
            .filter(|reported| reported.is_finite() && *reported > 0.0)
            .unwrap_or(self.label_kwh)
            .clamp(low, high.max(low))
    }

    fn band_at(&self, floor: f64) -> (f64, f64) {
        if !self.is_usable() {
            return (0.0, 0.0);
        }
        let top = self.label_kwh * (1.0 + UPWARD_FRACTION);
        let ceiling = self
            .reported_kwh
            .filter(|reported| reported.is_finite() && *reported > 0.0)
            .map_or(top, |reported| reported.min(top));
        if floor > ceiling {
            (ceiling, ceiling)
        } else {
            (floor, ceiling)
        }
    }
}

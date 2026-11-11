//! One battery pack: identity, capacity labels and the device power maxima.

/// A SolarFlow pack row.
///
/// `label_capacity_kwh` is the pack type's nameplate (AB2000 = 1.92 kWh), `reported_capacity_kwh`
/// is what the pack itself reports (if anything), and the maxima are the device limits known from
/// discovery. `first_seen` is preserved by the upsert; `last_seen` moves forward.
///
/// `name` is what discovery registered (Home Assistant's device name); `alias` is the operator's
/// own label, written only through the pack alias endpoint, so a re-registration never overwrites
/// it. The `id` is assigned by the database: it is ignored when a pack is upserted and returned on
/// the stored row.
#[derive(Debug, Clone, PartialEq)]
pub struct Pack {
    pub id: i64,
    pub sn: String,
    pub name: Option<String>,
    /// The operator's label; `None` when nobody renamed the pack.
    pub alias: Option<String>,
    pub label_capacity_kwh: f64,
    pub reported_capacity_kwh: Option<f64>,
    pub max_charge_w: Option<f64>,
    pub max_discharge_w: Option<f64>,
    pub first_seen: i64,
    pub last_seen: i64,
    /// The learner's effective capacity, kWh (written by a learning pass, `None` before one).
    pub learned_capacity_kwh: Option<f64>,
    /// The learner's AC round-trip efficiency.
    pub learned_round_trip: Option<f64>,
    /// Confidence of the learned numbers, `[0, 1]`.
    pub learned_confidence: Option<f64>,
    /// When the learned numbers were written, UTC epoch seconds.
    pub learned_at: Option<i64>,
}

impl Pack {
    /// A pack as first discovered (no reported capacity, no learned limits yet).
    pub fn new(sn: impl Into<String>, label_capacity_kwh: f64, first_seen: i64) -> Self {
        Self {
            id: 0,
            sn: sn.into(),
            name: None,
            alias: None,
            label_capacity_kwh,
            reported_capacity_kwh: None,
            max_charge_w: None,
            max_discharge_w: None,
            first_seen,
            last_seen: first_seen,
            learned_capacity_kwh: None,
            learned_round_trip: None,
            learned_confidence: None,
            learned_at: None,
        }
    }

    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[must_use]
    pub fn with_limits(mut self, charge_w: f64, discharge_w: f64) -> Self {
        self.max_charge_w = Some(charge_w);
        self.max_discharge_w = Some(discharge_w);
        self
    }

    #[must_use]
    pub fn with_reported_capacity(mut self, reported_capacity_kwh: f64) -> Self {
        self.reported_capacity_kwh = Some(reported_capacity_kwh);
        self
    }

    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            sn: row.get(1)?,
            name: row.get(2)?,
            alias: row.get(3)?,
            label_capacity_kwh: row.get(4)?,
            reported_capacity_kwh: row.get(5)?,
            max_charge_w: row.get(6)?,
            max_discharge_w: row.get(7)?,
            first_seen: row.get(8)?,
            last_seen: row.get(9)?,
            learned_capacity_kwh: row.get(10)?,
            learned_round_trip: row.get(11)?,
            learned_confidence: row.get(12)?,
            learned_at: row.get(13)?,
        })
    }
}

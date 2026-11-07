//! Which measurement table a read or an extraction uses.

/// The two stored measurement tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Raw 1-minute samples, kept for 30 days.
    OneMinute,
    /// 5-minute means rolled up from the 1-minute tier, kept for a year.
    FiveMinute,
}

impl Tier {
    pub fn interval_s(self) -> i64 {
        match self {
            Self::OneMinute => 60,
            Self::FiveMinute => 300,
        }
    }

    pub fn table(self) -> &'static str {
        match self {
            Self::OneMinute => "measurements",
            Self::FiveMinute => "measurements_5m",
        }
    }
}

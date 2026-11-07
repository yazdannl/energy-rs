//! Slot length used by the engine.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    FiveMinute,
    QuarterHour,
    Hour,
}

impl Granularity {
    pub fn millis(self) -> i64 {
        match self {
            Self::FiveMinute => 5 * 60 * 1_000,
            Self::QuarterHour => 15 * 60 * 1_000,
            Self::Hour => 60 * 60 * 1_000,
        }
    }

    pub(crate) fn source_period(self) -> super::Period {
        match self {
            Self::FiveMinute | Self::QuarterHour => super::Period::FiveMinute,
            Self::Hour => super::Period::Hour,
        }
    }
}

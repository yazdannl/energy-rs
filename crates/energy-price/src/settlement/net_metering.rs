//! The net-metering (saldering) switch: date-driven by default, with a manual override for a
//! supplier that still nets under a legacy contract.  It flips off on 2027-01-01 local time.

use chrono::{Datelike, NaiveDate};

/// The first local year without net metering (saldering stopped on 2027-01-01 in one step).
pub const SALDERING_END_YEAR: i32 = 2027;

/// How the regime is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetMetering {
    /// `local_date.year() < SALDERING_END_YEAR` (2026 and earlier: on; 2027+: off).
    #[default]
    DateDriven,
    /// Force 1:1 netting on.
    On,
    /// Force 1:1 netting off.
    Off,
}

impl NetMetering {
    /// Whether 1:1 netting applies on a local tariff date.
    pub fn is_active(self, local_date: NaiveDate) -> bool {
        match self {
            NetMetering::On => true,
            NetMetering::Off => false,
            NetMetering::DateDriven => local_date.year() < SALDERING_END_YEAR,
        }
    }
}

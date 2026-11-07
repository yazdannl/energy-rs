//! Nominal pack capacity from the pack serial prefix and `packType`.

/// What a pack is, and how much energy its label claims.
///
/// The device publishes no kWh figure at all, so this table is the only source of a capacity before
/// learning refines it. Only two rows are hardware-confirmed: `I8000` (`packType` 70) and
/// `AB2000` = 1.92 kWh (the live fleet's 1.92 kWh matches the AB2000 row exactly).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackLabel {
    /// Model name as the vendor integration labels it.
    pub model: &'static str,
    /// Nominal capacity in kWh; usable capacity is measured, not this number.
    pub capacity_kwh: f64,
    /// True for the rows confirmed on hardware, false for vendor-table rows.
    pub confirmed: bool,
}

impl PackLabel {
    /// Label a pack from the first and fourth serial characters plus `packType`.
    pub fn for_pack(sn: &str, pack_type: Option<i32>) -> Option<Self> {
        let mut chars = sn.chars();
        let prefix = chars.next()?.to_ascii_uppercase();
        let fourth = sn.chars().nth(3).map(|raw| raw.to_ascii_uppercase());
        let label = match prefix {
            'A' if fourth == Some('3') => Self::table("AIO2400", 2.4),
            'A' => Self::table("AB1000", 0.96),
            'B' if pack_type == Some(70) => Self::hardware("I8000", 8.0),
            'B' => Self::table("AB1000S", 0.96),
            'C' => Self::ab2000(fourth),
            'F' => Self::table("AB3000", 2.88),
            'G' => Self::table("AB3000L", 2.88),
            'J' => Self::table("I2400", 2.4),
            _ => return None,
        };
        Some(label)
    }

    fn ab2000(fourth: Option<char>) -> Self {
        match fourth {
            Some('F') => Self::table("AB2000S", 1.92),
            Some('E') => Self::table("AB2000X", 1.92),
            _ => Self::hardware("AB2000", 1.92),
        }
    }

    const fn table(model: &'static str, capacity_kwh: f64) -> Self {
        Self {
            model,
            capacity_kwh,
            confirmed: false,
        }
    }

    const fn hardware(model: &'static str, capacity_kwh: f64) -> Self {
        Self {
            model,
            capacity_kwh,
            confirmed: true,
        }
    }
}

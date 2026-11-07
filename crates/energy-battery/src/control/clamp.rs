//! Clamp a command to the config ceiling, the model's hard ceiling and the device's live maxima.

use crate::control::command::Command;
use crate::control::direction::Direction;
use crate::control::limits::ControlLimits;
use crate::error::BatteryError;
use crate::report::Report;

/// A power ceiling for one direction, with "not reported" kept apart from a real number.
///
/// The protocol reports 0 for "no value", never "no power", so a reported 0 must not become a
/// ceiling of 0 W and must not become "unlimited" either. An unknown ceiling stays unknown, and a
/// nonzero command with nothing trustworthy to bound it is refused instead of transmitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ceiling {
    /// A real bound in watts. Always > 0: a 0 W bound would mean "drive nothing", which neither the
    /// config nor a report can express.
    Known(u32),
    /// Nothing bounds this direction: a nonzero command cannot be sent.
    Unknown,
}

impl Ceiling {
    /// A ceiling read from configuration or a report: 0 is "unset"/"not reported", anything else is
    /// a real bound.
    pub const fn from_watts(watts: u32) -> Self {
        if watts == 0 {
            Self::Unknown
        } else {
            Self::Known(watts)
        }
    }

    /// The tighter of two ceilings; a known bound always beats an unknown one.
    #[must_use]
    pub const fn tighter(self, other: Self) -> Self {
        match (self, other) {
            (Self::Known(a), Self::Known(b)) => Self::Known(if a < b { a } else { b }),
            (Self::Known(a), Self::Unknown) | (Self::Unknown, Self::Known(a)) => Self::Known(a),
            (Self::Unknown, Self::Unknown) => Self::Unknown,
        }
    }

    pub const fn watts(self) -> Option<u32> {
        match self {
            Self::Known(watts) => Some(watts),
            Self::Unknown => None,
        }
    }
}

/// `requested` clamped to `ceiling`, or `None` when the request is nonzero and nothing bounds it.
///
/// A request of exactly 0 W is always acceptable (a hold, and a 0 W trim): it drives nothing, so it
/// needs no ceiling either.
pub fn clamp_watts(requested: u32, ceiling: Ceiling) -> Option<u32> {
    if requested == 0 {
        return Some(0);
    }
    ceiling.watts().map(|max| requested.min(max))
}

/// The tightest trustworthy ceiling for `direction`: the operator's config, the model's validated
/// hard ceiling and the device's live maximum, whichever are known.
///
/// A live maximum is the truth on this hardware, so it only ever lowers the ceiling; the config and
/// the hard ceiling are what is left when the device reports nothing.
pub fn ceiling_for(
    limits: &ControlLimits,
    report: Option<&Report>,
    direction: Direction,
) -> Ceiling {
    let mut ceiling = Ceiling::from_watts(limits.for_direction(direction));
    if let Some(hard) = limits.hard {
        ceiling = ceiling.tighter(Ceiling::from_watts(hard.for_direction(direction)));
    }
    if let Some(report) = report {
        ceiling = ceiling.tighter(Ceiling::from_watts(device_max(direction, report)));
    }
    ceiling
}

/// Clamp a command against the config, the hard ceiling and the last report's live maximum.
///
/// A nonzero command with no trustworthy ceiling is an error, not a pass-through: unbounded power is
/// the failure this whole layer exists to prevent. A hold, and a command already at 0 W, pass
/// unchanged.
///
/// # Errors
///
/// [`BatteryError::NoCeiling`] when a nonzero command has no bound at all.
pub fn clamp_command(
    command: Command,
    limits: &ControlLimits,
    report: Option<&Report>,
) -> Result<Command, BatteryError> {
    let (Some(direction), Some(watts)) = (command.direction(), command.watts()) else {
        return Ok(command);
    };
    clamp_watts(watts, ceiling_for(limits, report, direction))
        .map(|clamped| command.with_watts(clamped))
        .ok_or_else(|| BatteryError::NoCeiling {
            sn: report.map_or_else(|| "<unread>".to_owned(), |report| report.sn.clone()),
            direction,
            requested_w: watts,
        })
}

fn device_max(direction: Direction, report: &Report) -> u32 {
    match direction {
        Direction::Charge => report.properties.charge_max_w,
        Direction::Discharge => report.properties.inverse_max_w,
    }
}

//! A pack plus its newest 1-minute sample, which is what the "Now" screen and the engine need.

use crate::measurement::Measurement;
use crate::pack::Pack;

/// The latest known state of one pack. `last` is `None` until the first sample is ingested.
#[derive(Debug, Clone, PartialEq)]
pub struct PackState {
    pub pack: Pack,
    pub last: Option<Measurement>,
}

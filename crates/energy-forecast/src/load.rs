//! Load profile builder.
//!
//! A 48-bucket (half-hourly) profile per weekday class from HA long-term statistics, a
//! night-derived consumption tier, optional named-device subtraction, and an EWMA online update so
//! recent behaviour dominates without losing the shape.
//!
//! No code yet: P0 scaffold only.

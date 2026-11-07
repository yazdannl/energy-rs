//! One command the sink received.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_possible_wrap
)]
#![allow(dead_code)]

/// One command the sink received.
#[derive(Debug, Clone, PartialEq)]
pub enum Sent {
    Write(f64),
    Hold,
}

//! Transport-level write result, distinct from device echo and measured flow.

use crate::dispatch::Echo;

/// Result of one bounded transport write.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteAck {
    /// Transport accepted the command; echo may still be absent.
    Accepted { echo: Option<Echo>, at_ms: i64 },
    /// Device explicitly refused the command.
    Rejected { reason: &'static str, at_ms: i64 },
    /// Transport failed before acceptance.
    Failed { reason: &'static str, at_ms: i64 },
}

impl WriteAck {
    /// Whether the transport accepted the command.
    pub fn accepted(&self) -> bool {
        matches!(self, Self::Accepted { .. })
    }

    /// Echo carried by an accepted write, if any.
    pub fn echo(&self) -> Option<Echo> {
        match self {
            Self::Accepted { echo, .. } => *echo,
            _ => None,
        }
    }
}

//! What the fake server answers with.

use serde_json::Value;

/// One canned answer: a result, an error, follow-up events, or a close.
#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub result: Value,
    pub error: Option<(String, String)>,
    pub events: Vec<Value>,
    pub close_after: bool,
    pub silent: bool,
}

impl Reply {
    /// A successful result.
    pub fn ok(result: Value) -> Self {
        Self {
            result,
            ..Self::default()
        }
    }

    /// A Home Assistant error payload.
    pub fn fail(code: &str, message: &str) -> Self {
        Self {
            error: Some((code.to_owned(), message.to_owned())),
            ..Self::default()
        }
    }

    /// Send one event after the result.
    pub fn with_event(mut self, event: Value) -> Self {
        self.events.push(event);
        self
    }

    /// Close the socket after answering.
    pub fn close_after(mut self) -> Self {
        self.close_after = true;
        self
    }

    /// Answer nothing (for timeouts).
    pub fn silent() -> Self {
        Self {
            silent: true,
            ..Self::default()
        }
    }
}

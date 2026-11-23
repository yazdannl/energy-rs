//! One-time notices, represented as typed keys instead of log side effects.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoticeLog {
    /// Keys already emitted, in first-emission order.
    pub emitted: Vec<String>,
}

impl NoticeLog {
    /// Emit a key once; further calls with the same key change nothing.
    pub fn emit_once(&mut self, key: &str) -> bool {
        if self.emitted.iter().any(|old| old == key) {
            return false;
        }
        self.emitted.push(key.to_owned());
        true
    }
}

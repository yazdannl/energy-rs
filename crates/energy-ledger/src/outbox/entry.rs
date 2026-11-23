//! One committed but not-yet-journalled learning audit event.

/// A row in `learning_outbox`. Its id is the replay cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingLearningEvent {
    pub id: i64,
    pub pack_id: String,
    pub at: i64,
    pub summary: String,
}

impl PendingLearningEvent {
    pub fn new(id: i64, pack_id: String, at: i64, summary: String) -> Self {
        Self {
            id,
            pack_id,
            at,
            summary,
        }
    }
}

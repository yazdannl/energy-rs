//! Authority ledger value: events, checkpoint and latch. External storage replaces it wholesale.

use crate::authority::{
    AuthorityEvent, AuthorityEventKind, AuthorityState, BootstrapReason, Latch, LatchToken, journal,
};

/// Ledger state presented at startup. The real SQLite ledger is owned outside this crate.
#[derive(Debug, Clone)]
pub struct AuthorityLedger {
    schema_version: u32,
    events: Vec<AuthorityEvent>,
    checkpoint_event_id: u64,
    latch: Option<Latch>,
    persisted_mode: Option<String>,
    source_release: String,
}

impl AuthorityLedger {
    /// Empty ledger for a fresh install.
    pub fn new(schema_version: u32, persisted_mode: Option<&str>, source_release: &str) -> Self {
        Self {
            schema_version,
            events: Vec::new(),
            checkpoint_event_id: 0,
            latch: None,
            persisted_mode: persisted_mode.map(str::to_owned),
            source_release: source_release.to_owned(),
        }
    }

    /// Storage seam: reconstruct a ledger exactly as found on disk, including damaged heads.
    pub fn from_parts(
        schema_version: u32,
        events: Vec<AuthorityEvent>,
        checkpoint_event_id: u64,
        latch: Option<Latch>,
        persisted_mode: Option<String>,
        source_release: String,
    ) -> Self {
        Self {
            schema_version,
            events,
            checkpoint_event_id,
            latch,
            persisted_mode,
            source_release,
        }
    }

    /// Schema version recorded in the ledger.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
    /// Current durable head event id.
    pub fn head_event_id(&self) -> u64 {
        self.events.last().map_or(0, |event| event.id)
    }
    /// Authority-owned checkpoint; it advances atomically with every append.
    pub fn checkpoint_event_id(&self) -> u64 {
        self.checkpoint_event_id
    }
    /// Durable latch row, when one has been committed.
    pub fn latch(&self) -> Option<&Latch> {
        self.latch.as_ref()
    }
    /// All durable authority events.
    pub fn events(&self) -> &[AuthorityEvent] {
        &self.events
    }
    /// Persisted non-authoritative operational mode, if any.
    pub fn persisted_mode(&self) -> Option<&str> {
        self.persisted_mode.as_deref()
    }
    /// Source release recorded with a legacy ledger.
    pub fn source_release(&self) -> &str {
        &self.source_release
    }
    /// Chain head of the last event, or zero on an empty ledger.
    pub fn chain_head(&self) -> u64 {
        self.events.last().map_or(0, |event| event.hash)
    }

    /// True when checkpoint, head and the full hash chain are coherent.
    pub fn journal_complete(&self) -> bool {
        self.checkpoint_event_id == self.head_event_id() && journal::verify_chain(&self.events)
    }

    /// True when any latch/clearance history exists, which blocks legacy migration.
    pub fn has_latch_history(&self) -> bool {
        self.events.iter().any(|event| {
            matches!(
                event.kind,
                AuthorityEventKind::LatchTokenInitialized { .. }
                    | AuthorityEventKind::AuthorityTransition { .. }
                    | AuthorityEventKind::ClearanceEntry { .. }
                    | AuthorityEventKind::BarrierResult { .. }
            )
        }) || self.latch.is_some()
    }

    /// True when an initialization event explains this generation.
    pub fn explains_generation(&self, generation: Option<u64>) -> bool {
        generation.is_some_and(|generation| {
            self.events.iter().any(|event| {
                matches!(
                    event.kind,
                    AuthorityEventKind::LatchTokenInitialized {
                        generation: explained,
                        ..
                    } if explained == generation
                )
            })
        })
    }

    /// True when any initialization event exists, regardless of generation.
    pub fn has_initialization_event(&self) -> bool {
        self.events
            .iter()
            .any(|event| matches!(event.kind, AuthorityEventKind::LatchTokenInitialized { .. }))
    }

    /// Find the bootstrap reason explaining a generation, for operator recovery display.
    pub fn initialization_reason(&self, generation: u64) -> Option<BootstrapReason> {
        self.events.iter().find_map(|event| match event.kind {
            AuthorityEventKind::LatchTokenInitialized {
                generation: explained,
                reason,
                ..
            } if explained == generation => Some(reason),
            _ => None,
        })
    }

    /// Append one event and advance the checkpoint in the same value transaction.
    pub(crate) fn append(
        &mut self,
        at_ms: i64,
        actor: &str,
        why: &str,
        kind: AuthorityEventKind,
    ) -> u64 {
        let id = self.head_event_id().saturating_add(1);
        let mut event = AuthorityEvent::new(id, at_ms, actor, why, kind);
        event.prev_hash = self.chain_head();
        event.hash = journal::hash_event(event.prev_hash, &event);
        self.events.push(event);
        self.checkpoint_event_id = id;
        id
    }

    /// Install a latch row before any settings or ledger mutation.
    pub(crate) fn set_latch(&mut self, latch: Latch) {
        self.latch = Some(latch);
    }

    /// Change the legacy schema marker in the sealed bootstrap transaction.
    pub(crate) fn set_schema_version(&mut self, schema_version: u32) {
        self.schema_version = schema_version;
    }

    /// Storage seam: seed a non-latch journal event to test legacy provenance.
    pub fn seed_journal_note(&mut self, at_ms: i64, text: &str) {
        self.append(
            at_ms,
            "wiring",
            text,
            crate::authority::AuthorityEventKind::JournalNote {
                text: text.to_owned(),
            },
        );
    }

    /// Storage seam: simulate a database edit that removes the token row.
    pub fn remove_latch(&mut self) {
        self.latch = None;
    }

    /// Storage seam: simulate a manual edit of the persisted generation text.
    pub fn corrupt_generation(&mut self, raw: &str) {
        if let Some(latch) = self.latch.as_mut() {
            latch.corrupt_generation(raw);
        }
    }

    /// Storage seam: simulate an external database-only rollback to another snapshot.
    pub fn restore_snapshot(&mut self, snapshot: &Self) {
        *self = snapshot.clone();
    }

    /// Storage seam: simulate a stale authority checkpoint left by a database edit.
    pub fn set_checkpoint(&mut self, checkpoint_event_id: u64) {
        self.checkpoint_event_id = checkpoint_event_id;
    }

    /// Storage seam: simulate a truncated tail after appends.
    pub fn truncate_tail(&mut self, keep: usize) {
        self.events.truncate(keep);
    }

    /// Mint the token generation used by a bootstrap, from the current head.
    pub(crate) fn seed(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.source_release,
            self.schema_version,
            self.persisted_mode.as_deref().unwrap_or("missing"),
            self.head_event_id()
        )
    }

    /// Build the latch for a bootstrap result.
    pub(crate) fn initial_latch(
        &self,
        generation: u64,
        state: AuthorityState,
        reason: &str,
        at_ms: i64,
    ) -> Latch {
        Latch::new(
            LatchToken::mint(generation, &self.seed()),
            state,
            reason,
            at_ms,
        )
    }
}

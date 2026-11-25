//! Auditable caller identity; the authority-internal capability cannot be forged by an API caller.

use crate::authority::EntryPoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Capability {
    Operator,
    Engine,
    AuthorityInternal,
}

/// Who asked, through which entry point, why and when.
#[derive(Debug, Clone)]
pub struct Caller {
    actor: String,
    entry: EntryPoint,
    at_ms: i64,
    why: String,
    capability: Capability,
}

impl Caller {
    /// Operator-facing request: can never carry authority-internal capability.
    pub fn operator(entry: EntryPoint, actor: &str, at_ms: i64, why: &str) -> Self {
        Self {
            actor: actor.to_owned(),
            entry,
            at_ms,
            why: why.to_owned(),
            capability: Capability::Operator,
        }
    }

    /// Ordinary engine path; still not authority-internal.
    pub fn system(entry: EntryPoint, at_ms: i64, why: &str) -> Self {
        Self {
            actor: "engine".to_owned(),
            entry,
            at_ms,
            why: why.to_owned(),
            capability: Capability::Engine,
        }
    }

    /// Only crate-internal authority code can build this caller.
    pub(crate) fn internal(entry: EntryPoint, at_ms: i64, why: &str) -> Self {
        Self {
            actor: "authority".to_owned(),
            entry,
            at_ms,
            why: why.to_owned(),
            capability: Capability::AuthorityInternal,
        }
    }

    /// Actor recorded in the audit row.
    pub fn actor(&self) -> &str {
        &self.actor
    }
    /// Entry point recorded in the audit row.
    pub fn entry(&self) -> EntryPoint {
        self.entry
    }
    /// Instant of the request, epoch milliseconds.
    pub fn at_ms(&self) -> i64 {
        self.at_ms
    }
    /// Why the request was made.
    pub fn why(&self) -> &str {
        &self.why
    }
    /// True only for the sealed authority-internal capability.
    pub fn is_authority_internal(&self) -> bool {
        self.capability == Capability::AuthorityInternal
    }
}

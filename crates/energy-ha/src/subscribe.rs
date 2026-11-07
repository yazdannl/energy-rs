//! Live state subscriptions.
//!
//! One WS subscription per watched entity set, deduplicated and coalesced into the slot
//! measurement stream the engine consumes. Drops are reported as gaps, never interpolated.
//!
//! No code yet: P0 scaffold only.

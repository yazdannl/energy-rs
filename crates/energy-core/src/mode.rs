//! Control mode.
//!
//! `advisory` computes and stores plans only, `dry_run` runs the full pipeline including clamps and
//! logs the intended writes, `auto` actually writes. Default is `dry_run`; the mode is persisted
//! and changes require an explicit setting write.
//!
//! No code yet: P0 scaffold only.

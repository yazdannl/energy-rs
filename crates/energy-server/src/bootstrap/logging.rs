//! One tracing subscriber, filtered by the add-on's log level.

use tracing_subscriber::EnvFilter;

/// Initialise stdout logging from `LOG_LEVEL` (set by `run.sh`) or the options file.
///
/// The add-on option is a plain level (`trace`..`error`); `ENERGY_RS_LOG`/`RUST_LOG` still work for
/// a manual `docker run`, and an unparsable filter falls back to `info` instead of refusing to start.
pub fn init(level: &str) {
    let directives = std::env::var("LOG_LEVEL")
        .or_else(|_| std::env::var("ENERGY_RS_LOG"))
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| level.to_owned());
    let filter = EnvFilter::try_new(&directives).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

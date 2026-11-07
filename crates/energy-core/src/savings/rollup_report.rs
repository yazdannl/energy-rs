//! What one rollup pass wrote.

/// The outcome of one savings rollup pass, for the boot log and the tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RollupReport {
    /// Local days recomputed, ascending ISO dates.
    pub days: Vec<String>,
    /// Closed days whose measurements covered the whole day.
    pub complete: usize,
    /// Closed days stored with `NULL` money because coverage was incomplete.
    pub incomplete: usize,
    /// Whether the provisional current day was written.
    pub provisional: bool,
    /// Days that could not be read, with their error (the pass keeps going).
    pub failures: Vec<String>,
}

impl RollupReport {
    /// Count one stored day.
    pub(crate) fn record(&mut self, day: &str, complete: bool, closed: bool) {
        self.days.push(day.to_owned());
        if !closed {
            self.provisional = true;
        } else if complete {
            self.complete += 1;
        } else {
            self.incomplete += 1;
        }
    }
}

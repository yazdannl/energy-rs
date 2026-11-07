//! The collected checks: exit code, table rendering and the next steps for a failure.

use std::fmt::Write as _;

use serde::Serialize;

use crate::diag::check::Check;
use crate::diag::status::Status;
use crate::diag::table;

/// Every check of one diagnostic run.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub checks: Vec<Check>,
    /// Per-phase wall time in milliseconds; only populated with `--verbose`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub timings: Vec<(String, u64)>,
}

impl Report {
    pub fn push(&mut self, check: Check) {
        self.checks.push(check);
    }

    pub fn extend(&mut self, checks: impl IntoIterator<Item = Check>) {
        self.checks.extend(checks);
    }

    /// 0 when everything passed, 1 for warnings only, 2 for at least one failure.
    pub fn exit_code(&self) -> i32 {
        match (self.has(Status::Fail), self.has(Status::Warn)) {
            (true, _) => 2,
            (false, true) => 1,
            (false, false) => 0,
        }
    }

    /// Whether any check carries exactly this status.
    pub fn has(&self, status: Status) -> bool {
        self.checks.iter().any(|check| check.status == status)
    }

    /// The failed checks, in order.
    pub fn failures(&self) -> impl Iterator<Item = &Check> {
        self.checks
            .iter()
            .filter(|check| check.status == Status::Fail)
    }

    fn count(&self, status: Status) -> usize {
        self.checks
            .iter()
            .filter(|check| check.status == status)
            .count()
    }

    /// The table plus a count line, the timings (`--verbose`) and the next steps for failures.
    pub fn render(&self) -> String {
        let mut rows = vec![vec![
            "STATUS".to_owned(),
            "CHECK".to_owned(),
            "DETAIL".to_owned(),
        ]];
        rows.extend(self.checks.iter().map(|check| {
            vec![
                check.status.label().to_owned(),
                check.name.clone(),
                check.detail.clone(),
            ]
        }));
        let widths = table::widths(&rows, 6);
        let mut out = String::new();
        for (index, row) in rows.iter().enumerate() {
            out.push_str(&table::line(row, &widths));
            out.push('\n');
            if index == 0 {
                let rule: String = widths
                    .iter()
                    .map(|width| "-".repeat(*width))
                    .collect::<Vec<_>>()
                    .join("  ");
                out.push_str(&rule);
                out.push('\n');
            }
        }
        let _ = writeln!(
            out,
            "\n{} check(s): {} pass, {} warn, {} fail",
            self.checks.len(),
            self.count(Status::Pass),
            self.count(Status::Warn),
            self.count(Status::Fail),
        );
        if !self.timings.is_empty() {
            out.push_str("timings:");
            for (phase, millis) in &self.timings {
                let _ = write!(out, " {phase} {millis} ms,");
            }
            out.push('\n');
        }
        let steps = self.next_steps();
        if !steps.is_empty() {
            out.push_str("\nnext steps:\n");
            for step in steps {
                let _ = writeln!(out, "  - {step}");
            }
        }
        out
    }

    /// One actionable line per failed check, deduplicated and in check order.
    pub fn next_steps(&self) -> Vec<String> {
        let mut steps: Vec<String> = Vec::new();
        for check in self.failures() {
            let advice = advice(&check.name);
            if !steps.iter().any(|step| step == advice) {
                steps.push(advice.to_owned());
            }
        }
        steps
    }
}

/// What to do about a failing check, keyed by the check's name prefix.
fn advice(name: &str) -> &'static str {
    if name == "ha transport" {
        "check the Home Assistant URL/token (SUPERVISOR_TOKEN, or HASS_URL and HASS_TOKEN); GET /api/config must answer"
    } else if name == "energy dashboard" {
        "configure the Energy dashboard (grid import/export, solar energy, battery) and restart the add-on"
    } else if name.starts_with("stats ") {
        "enable recorder statistics for the named entity and wait five minutes; the dashboard source must be a statistic"
    } else if name == "live meter" {
        "check that the dashboard's power entities exist and update; a missing grid power means no plan"
    } else if name == "prices store" {
        "start the add-on once so the price store exists and holds a publication"
    } else if name.starts_with("prices") {
        "check the price provider and its token; tomorrow's prices are published around 13:00 CET"
    } else if name.starts_with("battery") || name == "batteries" {
        "check the LAN address and serial (or mDNS); power-cycle a pack that reports a wedged acMode"
    } else if name.starts_with("ledger") {
        "start the add-on once so /data/energy_rs.db exists and is migrated"
    } else if name == "verdict" {
        "fix the failed checks above; the plan is held, never forced"
    } else {
        "see docs/DOCTOR.md for this row"
    }
}

//! What the efficiency estimator saw, tried and refused in one learning pass.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::de::{Deserializer, Visitor};
use serde::{Deserialize, Serialize};

use crate::learning::pair_attempt::PairAttempt;

/// Only the most recent attempts are kept: enough to explain a refusal, bounded for the API.
const MAX_ATTEMPTS: usize = 12;

/// Session and pair counts plus the refusal-reason distribution of one pass.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairDiagnostics {
    /// When the last scheduled or manual relearn pass examined this pack, including skips.
    #[serde(default)]
    pub last_attempt_at: Option<DateTime<Utc>>,
    /// When the learner last actually ran, including a backfill that found no measurement.
    ///
    /// Separate from `last_attempt_at` so frequent diagnostic skips cannot postpone the periodic
    /// backfill. A profile written before this field existed inherits `last_attempt_at` on read.
    #[serde(default)]
    pub last_learning_at: Option<DateTime<Utc>>,
    /// Ledger sessions in the learning window, whether or not they could be handed to the learner.
    #[serde(default)]
    pub sessions_in_window: usize,
    /// Window session records marked complete (bounded and with both SoC endpoints).
    #[serde(default)]
    pub sessions_complete: usize,
    /// Session records for this pack in the learning window, valid or not.
    #[serde(default)]
    pub sessions_seen: usize,
    /// Session records the learner could use after their own validity checks.
    #[serde(default)]
    pub sessions_valid: usize,
    /// Discharge blocks that were considered as the second leg of a cycle.
    #[serde(default)]
    pub candidates: usize,
    /// Candidates that produced a plausible measurement.
    #[serde(default)]
    pub accepted: usize,
    /// Candidates that did not.
    #[serde(default)]
    pub refused: usize,
    /// Same-direction boundaries where a continuity guard split a block.
    #[serde(default)]
    pub block_splits: usize,
    /// Counts by reason slug (candidate refusals and block splits).
    #[serde(default)]
    pub reasons: BTreeMap<String, usize>,
    /// Up to [`MAX_ATTEMPTS`] newest boundary splits and candidates, oldest first.
    #[serde(default)]
    pub attempts: Vec<PairAttempt>,
    /// Accepted pairs behind the current round-trip estimate.
    #[serde(default)]
    pub pairs: usize,
    /// Kish effective pair count: `(Σw)² / Σw²`, at most `pairs`.
    #[serde(default)]
    pub effective_pairs: f64,
    /// Median absolute deviation of the accepted pairs' round trips (one vote per couple).
    #[serde(default)]
    pub dispersion: f64,
    /// Confidence derived from this pass's own evidence (0 when application was blocked); the
    /// estimate's published confidence may instead be carried forward from a previous pass.
    #[serde(default)]
    pub confidence: f64,
    /// Why this pass did not apply a measurement: `too_few_pairs`, `too_few_effective_pairs`,
    /// `disagreement`, `no_measurement`, or a skip reason (`no_sessions_in_window`,
    /// `no_completed_session`, `no_new_completed_session`); `null` when it applied one.
    #[serde(default)]
    pub blocker: Option<String>,
}

/// The on-disk shape: every field optional so older profiles remain readable.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairDiagnosticsWire {
    #[serde(default)]
    last_attempt_at: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "explicit_learning_time")]
    last_learning_at: LearningTimeField,
    #[serde(default)]
    sessions_in_window: usize,
    #[serde(default)]
    sessions_complete: usize,
    #[serde(default)]
    sessions_seen: usize,
    #[serde(default)]
    sessions_valid: usize,
    #[serde(default)]
    candidates: usize,
    #[serde(default)]
    accepted: usize,
    #[serde(default)]
    refused: usize,
    #[serde(default)]
    block_splits: usize,
    #[serde(default)]
    reasons: BTreeMap<String, usize>,
    #[serde(default)]
    attempts: Vec<PairAttempt>,
    #[serde(default)]
    pairs: usize,
    #[serde(default)]
    effective_pairs: f64,
    #[serde(default)]
    dispersion: f64,
    #[serde(default)]
    confidence: f64,
    #[serde(default)]
    blocker: Option<String>,
}

/// Distinguish a missing field (migrate from `lastAttemptAt`) from an explicit JSON `null`.
#[derive(Debug, Default)]
enum LearningTimeField {
    #[default]
    Missing,
    Null,
    Value(DateTime<Utc>),
}

fn explicit_learning_time<'de, D>(deserializer: D) -> Result<LearningTimeField, D::Error>
where
    D: Deserializer<'de>,
{
    struct Field;

    impl<'de> Visitor<'de> for Field {
        type Value = LearningTimeField;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a timestamp or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(LearningTimeField::Null)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            DateTime::<Utc>::deserialize(deserializer).map(LearningTimeField::Value)
        }
    }

    deserializer.deserialize_option(Field)
}

impl<'de> Deserialize<'de> for PairDiagnostics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = PairDiagnosticsWire::deserialize(deserializer)?;
        let PairDiagnosticsWire {
            last_attempt_at,
            last_learning_at,
            sessions_in_window,
            sessions_complete,
            sessions_seen,
            sessions_valid,
            candidates,
            accepted,
            refused,
            block_splits,
            reasons,
            attempts,
            pairs,
            effective_pairs,
            dispersion,
            confidence,
            blocker,
        } = wire;
        Ok(Self {
            last_attempt_at,
            last_learning_at: match last_learning_at {
                LearningTimeField::Missing => last_attempt_at,
                LearningTimeField::Null => None,
                LearningTimeField::Value(value) => Some(value),
            },
            sessions_in_window,
            sessions_complete,
            sessions_seen,
            sessions_valid,
            candidates,
            accepted,
            refused,
            block_splits,
            reasons,
            attempts,
            pairs,
            effective_pairs,
            dispersion,
            confidence,
            blocker,
        })
    }
}

impl PairDiagnostics {
    /// A pass that looked at the window but deliberately did not learn the pack.
    ///
    /// This keeps `updatedAt` on the last real measurement: an attempt is not an observation.
    pub fn skipped(
        now: DateTime<Utc>,
        sessions_in_window: usize,
        sessions_complete: usize,
        blocker: &'static str,
    ) -> Self {
        Self {
            last_attempt_at: Some(now),
            sessions_in_window,
            sessions_complete,
            sessions_seen: sessions_in_window,
            blocker: Some(blocker.to_owned()),
            ..Self::default()
        }
    }

    /// Add one candidate to the counts and the bounded attempt list.
    pub fn record(&mut self, attempt: PairAttempt) {
        *self.reasons.entry(attempt.reason.clone()).or_insert(0) += 1;
        self.candidates += 1;
        if attempt.accepted {
            self.accepted += 1;
        } else {
            self.refused += 1;
        }
        if self.attempts.len() == MAX_ATTEMPTS {
            self.attempts.remove(0);
        }
        self.attempts.push(attempt);
    }

    /// Keep the newest boundary events and candidates in chronological order.
    pub fn set_attempts(&mut self, mut attempts: Vec<PairAttempt>) {
        attempts.sort_by_key(|attempt| attempt.at);
        self.attempts = attempts.into_iter().rev().take(MAX_ATTEMPTS).collect();
        self.attempts.reverse();
    }

    /// Record a same-direction boundary that failed a continuity guard.
    pub fn record_split(&mut self, reason: &'static str) {
        *self.reasons.entry(reason.to_owned()).or_insert(0) += 1;
        self.block_splits += 1;
    }

    /// Set the evidence behind the efficiency decision.
    pub fn set_evidence(
        &mut self,
        pairs: usize,
        effective_pairs: f64,
        dispersion: f64,
        confidence: f64,
        blocker: Option<&'static str>,
    ) {
        self.pairs = pairs;
        self.effective_pairs = effective_pairs;
        self.dispersion = dispersion;
        self.confidence = confidence;
        self.blocker = blocker.map(str::to_owned);
    }

    /// One line for the debug log and the capacity report.
    pub fn summary(&self) -> String {
        let reasons = self
            .reasons
            .iter()
            .map(|(reason, count)| format!("{reason}={count}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "last attempt {:?}, last learning {:?}, sessions {}/{} valid ({} in window, {} complete), candidates {}, accepted {}, refused {}, splits {}, pairs {} (eff {:.2}), dispersion {:.4}, confidence {:.3}, blocked {:?} [{reasons}]",
            self.last_attempt_at,
            self.last_learning_at,
            self.sessions_valid,
            self.sessions_seen,
            self.sessions_in_window,
            self.sessions_complete,
            self.candidates,
            self.accepted,
            self.refused,
            self.block_splits,
            self.pairs,
            self.effective_pairs,
            self.dispersion,
            self.confidence,
            self.blocker
        )
    }
}

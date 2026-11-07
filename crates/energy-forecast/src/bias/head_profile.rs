//! The persisted head payload, its promotion state and the revert path.

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use super::head_fit::{FACTOR_MAX, FACTOR_MIN};
use super::head_report::HeadReport;
use crate::ForecastError;
use crate::time::{iso_seconds, parse_iso_utc};

/// Deepest `previous` chain accepted from a payload; anything deeper is treated as corrupt.
const MAX_PREVIOUS_DEPTH: usize = 8;

/// The `forecast_head` learning payload: factors, the gate's numbers and the profile it replaced.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadProfile {
    /// `promoted`, `rejected`, `insufficient` or `reverted`.
    pub status: String,
    /// Whether these factors are in force.
    pub promoted: bool,
    /// The 24 per-local-hour factors.
    pub factors: [f64; 24],
    /// When the fit ran.
    pub fitted_at: Option<DateTime<Utc>>,
    /// The forecast kind this head corrects (`solar` or `load`).
    pub kind: String,
    /// Scored slots in the window.
    pub samples: usize,
    /// Scored slots used to fit.
    pub fit_samples: usize,
    /// Scored slots used to gate.
    pub holdout_samples: usize,
    /// Candidate holdout MAE, kW.
    pub candidate_mae_kw: Option<f64>,
    /// Incumbent holdout MAE, kW.
    pub incumbent_mae_kw: Option<f64>,
    /// Ridge prior strength used.
    pub shrinkage: f64,
    /// The gate's reason.
    pub reason: String,
    /// The profile this one replaced, kept so a promotion can be undone.
    pub previous: Option<Box<HeadProfile>>,
    /// When a revert ran, if it did.
    pub reverted_at: Option<DateTime<Utc>>,
}

impl HeadProfile {
    /// Build the payload a refit stores, from the gate's report and its factors.
    pub fn from_report(
        report: &HeadReport,
        factors: [f64; 24],
        kind: &str,
        now: DateTime<Utc>,
        previous: Option<HeadProfile>,
    ) -> Self {
        Self {
            status: report.status.as_str().to_string(),
            promoted: report.promoted,
            factors,
            fitted_at: Some(now),
            kind: kind.to_string(),
            samples: report.samples,
            fit_samples: report.fit_samples,
            holdout_samples: report.holdout_samples,
            candidate_mae_kw: report.candidate_mae_kw,
            incumbent_mae_kw: report.incumbent_mae_kw,
            shrinkage: super::head_fit::SHRINKAGE,
            reason: report.reason.clone(),
            previous: previous.map(Box::new),
            reverted_at: None,
        }
    }

    /// The factors a promoted head wants applied, or `None` when nothing is promoted.
    ///
    /// `None` rather than ones is deliberate: the caller must be able to tell "no head" from "a
    /// head that says the forecast is already right", because only the first falls back to the
    /// shipped EWMA correction. Every factor is clamped on read.
    pub fn active_factors(&self) -> Option<[f64; 24]> {
        if !self.promoted {
            return None;
        }
        let mut factors = [1.0_f64; 24];
        for (slot, factor) in factors.iter_mut().zip(self.factors.iter()) {
            *slot = factor.clamp(FACTOR_MIN, FACTOR_MAX);
        }
        Some(factors)
    }

    /// Undo the last promotion by restoring the previous profile. `true` when something changed.
    pub fn revert(&mut self, now: DateTime<Utc>) -> bool {
        let Some(previous) = self.previous.take() else {
            if !self.promoted {
                return false;
            }
            self.promoted = false;
            self.status = "reverted".to_string();
            self.reverted_at = Some(now);
            return true;
        };
        let mut restored = *previous;
        restored.promoted = false;
        restored.status = "reverted".to_string();
        restored.reverted_at = Some(now);
        *self = restored;
        true
    }

    /// Read a stored payload tolerantly.
    pub fn from_payload(payload: &Value) -> Result<Self, ForecastError> {
        Self::parse(payload, 0)
    }

    /// Serialise the payload, preserving the previous profile.
    pub fn to_payload(&self) -> Value {
        let mut map = Map::new();
        map.insert("status".to_string(), Value::String(self.status.clone()));
        map.insert("promoted".to_string(), Value::Bool(self.promoted));
        map.insert(
            "factors".to_string(),
            Value::Array(self.factors.iter().map(|value| number(*value)).collect()),
        );
        map.insert("fitted_at".to_string(), optional_time(self.fitted_at));
        map.insert("kind".to_string(), Value::String(self.kind.clone()));
        map.insert("samples".to_string(), Value::from(self.samples as u64));
        map.insert(
            "fit_samples".to_string(),
            Value::from(self.fit_samples as u64),
        );
        map.insert(
            "holdout_samples".to_string(),
            Value::from(self.holdout_samples as u64),
        );
        map.insert(
            "candidate_mae_kw".to_string(),
            self.candidate_mae_kw.map_or(Value::Null, number),
        );
        map.insert(
            "incumbent_mae_kw".to_string(),
            self.incumbent_mae_kw.map_or(Value::Null, number),
        );
        map.insert("shrinkage".to_string(), number(self.shrinkage));
        map.insert("reason".to_string(), Value::String(self.reason.clone()));
        map.insert(
            "previous".to_string(),
            self.previous
                .as_ref()
                .map_or(Value::Null, |profile| profile.to_payload()),
        );
        map.insert("reverted_at".to_string(), optional_time(self.reverted_at));
        Value::Object(map)
    }

    fn parse(payload: &Value, depth: usize) -> Result<Self, ForecastError> {
        if depth > MAX_PREVIOUS_DEPTH {
            return Err(ForecastError::Payload(
                "forecast head payload nests too deeply".to_string(),
            ));
        }
        let map = payload.as_object().ok_or_else(|| {
            ForecastError::Payload("forecast head payload is not a JSON object".to_string())
        })?;
        let mut factors = [1.0_f64; 24];
        if let Some(Value::Array(items)) = map.get("factors") {
            for (slot, item) in factors.iter_mut().zip(items.iter()) {
                *slot = item.as_f64().unwrap_or(1.0).clamp(FACTOR_MIN, FACTOR_MAX);
            }
        }
        let previous = match map.get("previous") {
            Some(Value::Object(_)) => Some(Box::new(Self::parse(
                map.get("previous").unwrap_or(&Value::Null),
                depth + 1,
            )?)),
            _ => None,
        };
        Ok(Self {
            status: text(map, "status"),
            promoted: map
                .get("promoted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            factors,
            fitted_at: time(map, "fitted_at"),
            kind: text(map, "kind"),
            samples: count(map, "samples"),
            fit_samples: count(map, "fit_samples"),
            holdout_samples: count(map, "holdout_samples"),
            candidate_mae_kw: map.get("candidate_mae_kw").and_then(Value::as_f64),
            incumbent_mae_kw: map.get("incumbent_mae_kw").and_then(Value::as_f64),
            shrinkage: map
                .get("shrinkage")
                .and_then(Value::as_f64)
                .unwrap_or(super::head_fit::SHRINKAGE),
            reason: text(map, "reason"),
            previous,
            reverted_at: time(map, "reverted_at"),
        })
    }
}

fn number(value: f64) -> Value {
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}

fn optional_time(value: Option<DateTime<Utc>>) -> Value {
    value.map_or(Value::Null, |moment| Value::String(iso_seconds(moment)))
}

fn text(map: &Map<String, Value>, key: &str) -> String {
    map.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn count(map: &Map<String, Value>, key: &str) -> usize {
    map.get(key)
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(usize::MAX as u64) as usize
}

fn time(map: &Map<String, Value>, key: &str) -> Option<DateTime<Utc>> {
    map.get(key).and_then(Value::as_str).and_then(parse_iso_utc)
}

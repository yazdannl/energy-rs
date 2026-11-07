//! How the per-slot source values are combined.

use super::merge_outcome::MergeOutcome;
use super::merge_row::MergeRow;
use super::source::SolarSource;
use super::state::SourceState;
use chrono::{DateTime, Utc};

/// How the per-slot values are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Weighted arithmetic mean (the default).
    WeightedMean,
    /// Middle value; the mean of the two middle values for an even count.
    Median,
    /// Largest value.
    Max,
}

impl MergeStrategy {
    /// Parse a configured strategy; anything unknown falls back to the weighted mean.
    pub fn parse(text: &str) -> Self {
        match text.trim().to_lowercase().as_str() {
            "max" => Self::Max,
            "median" => Self::Median,
            _ => Self::WeightedMean,
        }
    }

    /// The configured name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WeightedMean => "weighted_mean",
            Self::Median => "median",
            Self::Max => "max",
        }
    }
}

/// Merge every contributing source onto `grid`.
///
/// Ties are deterministic: `median` of an even count is the mean of the two middle values in the
/// sources' order. An empty result is 0.0 rather than an error, which is how the caller detects
/// that every source failed. A non-empty series still contributes even when its status is `failed`;
/// only the report row says `failed`.
pub fn merge_solar_series(
    grid: &[DateTime<Utc>],
    sources: &[SolarSource],
    strategy: MergeStrategy,
) -> MergeOutcome {
    let mut aligned: Vec<(&SolarSource, f64)> = Vec::new();
    let mut report = Vec::with_capacity(sources.len());
    for source in sources {
        let weight = source.safe_weight();
        if let Some(row) = skip_row(source, weight) {
            report.push(row);
            continue;
        }
        report.push(MergeRow {
            name: source.name.clone(),
            weight,
            state: source.state,
            note: source.note.clone(),
            points: source.series.len(),
        });
        aligned.push((source, weight));
    }
    let values = grid
        .iter()
        .map(|slot| {
            let picked: Vec<(f64, f64)> = aligned
                .iter()
                .map(|(source, weight)| (source.series.interpolate(*slot), *weight))
                .collect();
            merge_values(&picked, strategy).max(0.0)
        })
        .collect();
    MergeOutcome { values, report }
}

/// The report row for a source that does not contribute, or `None` when it does.
fn skip_row(source: &SolarSource, weight: f64) -> Option<MergeRow> {
    if source.state == SourceState::Disabled {
        return Some(MergeRow {
            name: source.name.clone(),
            weight,
            state: SourceState::Disabled,
            note: source.note.clone(),
            points: 0,
        });
    }
    if source.series.is_empty() {
        return Some(MergeRow {
            name: source.name.clone(),
            weight,
            state: SourceState::Failed,
            note: if source.note.is_empty() {
                "no usable forecast".to_string()
            } else {
                source.note.clone()
            },
            points: 0,
        });
    }
    None
}

/// Combine the per-slot values with `strategy`.
pub fn merge_values(picked: &[(f64, f64)], strategy: MergeStrategy) -> f64 {
    if picked.is_empty() {
        return 0.0;
    }
    match strategy {
        MergeStrategy::Max => picked
            .iter()
            .map(|(value, _)| *value)
            .fold(f64::NEG_INFINITY, f64::max),
        MergeStrategy::Median => median(picked),
        MergeStrategy::WeightedMean => weighted_mean(picked),
    }
}

fn median(picked: &[(f64, f64)]) -> f64 {
    let mut ordered: Vec<f64> = picked.iter().map(|(value, _)| *value).collect();
    ordered.sort_by(f64::total_cmp);
    let middle = ordered.len() / 2;
    if ordered.len() % 2 == 1 {
        return ordered[middle];
    }
    // Python computes `(a + b) / 2.0`; `f64::midpoint` is correctly rounded and can differ by one
    // ULP, which the golden merge fixture compares against.
    #[allow(clippy::manual_midpoint)]
    {
        (ordered[middle - 1] + ordered[middle]) / 2.0
    }
}

fn weighted_mean(picked: &[(f64, f64)]) -> f64 {
    let total: f64 = picked.iter().map(|(_, weight)| *weight).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let weighted: f64 = picked.iter().map(|(value, weight)| value * weight).sum();
    weighted / total
}

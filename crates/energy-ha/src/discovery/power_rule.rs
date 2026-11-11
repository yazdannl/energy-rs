//! How to obtain live signed power in W from entity states.

use std::collections::HashMap;

use super::{PowerConfig, PowerReading};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowerRule {
    /// Use the sensor as-is.
    Direct(String),
    /// Negate the sensor (Core `stat_rate_inverted`).
    Inverted(String),
    /// `from - to`, each converted to W first. Grid: import − export. Battery: discharge − charge.
    Difference { from: String, to: String },
    /// Sum several sources. Strict: any missing child makes the whole result missing, so a partial
    /// array is never silently under-counted.
    Sum(Vec<PowerRule>),
    /// Prefer the configured `stat_rate`; if it has no reading, recompute from `power_config`.
    Preferred {
        primary: Box<PowerRule>,
        fallback: Box<PowerRule>,
    },
}

impl PowerRule {
    /// Prefer the configured `stat_rate`; otherwise resolve the `power_config` variant.
    pub fn from_source(
        stat_rate: Option<&str>,
        power_config: Option<&PowerConfig>,
    ) -> Option<Self> {
        let primary = stat_rate
            .filter(|value| !value.is_empty())
            .map(|value| Self::Direct(value.to_owned()));
        let fallback = power_config.and_then(Self::from_config);
        match (primary, fallback) {
            (Some(primary), Some(fallback)) if primary != fallback => Some(Self::Preferred {
                primary: Box::new(primary),
                fallback: Box::new(fallback),
            }),
            (Some(primary), _) => Some(primary),
            (None, fallback) => fallback,
        }
    }

    fn from_config(config: &PowerConfig) -> Option<Self> {
        if let Some(stat_rate) = config
            .stat_rate
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            return Some(Self::Direct(stat_rate.to_owned()));
        }
        if let Some(stat_rate) = config
            .stat_rate_inverted
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            return Some(Self::Inverted(stat_rate.to_owned()));
        }
        let from = config
            .stat_rate_from
            .as_deref()
            .filter(|value| !value.is_empty())?;
        let to = config
            .stat_rate_to
            .as_deref()
            .filter(|value| !value.is_empty())?;
        Some(Self::Difference {
            from: from.to_owned(),
            to: to.to_owned(),
        })
    }

    pub fn entities(&self) -> Vec<&str> {
        match self {
            Self::Direct(entity) | Self::Inverted(entity) => vec![entity.as_str()],
            Self::Difference { from, to } => vec![from.as_str(), to.as_str()],
            Self::Sum(rules) => rules.iter().flat_map(Self::entities).collect(),
            Self::Preferred { primary, fallback } => {
                let mut entities = primary.entities();
                entities.extend(fallback.entities());
                entities
            }
        }
    }

    /// Signed watts, or `None` when any required reading is missing or has an unknown unit.
    pub fn resolve(&self, readings: &HashMap<String, PowerReading>) -> Option<f64> {
        match self {
            Self::Direct(entity) => readings.get(entity)?.watts(),
            Self::Inverted(entity) => Some(-readings.get(entity)?.watts()?),
            Self::Difference { from, to } => {
                Some(readings.get(from)?.watts()? - readings.get(to)?.watts()?)
            }
            Self::Sum(rules) => {
                let mut total = 0.0;
                for rule in rules {
                    total += rule.resolve(readings)?;
                }
                Some(total)
            }
            Self::Preferred { primary, fallback } => primary
                .resolve(readings)
                .or_else(|| fallback.resolve(readings)),
        }
    }
    /// Append every entity id this rule reads.
    pub fn collect_ids(&self, ids: &mut Vec<String>) {
        match self {
            Self::Direct(id) | Self::Inverted(id) => ids.push(id.clone()),
            Self::Difference { from, to } => {
                ids.push(from.clone());
                ids.push(to.clone());
            }
            Self::Sum(rules) => {
                for rule in rules {
                    rule.collect_ids(ids);
                }
            }
            Self::Preferred { primary, fallback } => {
                primary.collect_ids(ids);
                fallback.collect_ids(ids);
            }
        }
    }
}

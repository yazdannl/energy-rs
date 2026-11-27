//! The type of a writable setting: how it is validated and rendered.

use serde_json::{Value, json};

/// The longest accepted text value.
///
/// Entity ids and labels are short, but `pv_planes` is a JSON array of the array's roof planes and a
/// three-plane list is already ~230 characters.
pub(crate) const TEXT_MAX: usize = 4_096;

/// What a setting value may be.
#[derive(Debug, Clone, Copy)]
pub(crate) enum SettingKind {
    /// One of a fixed set of slugs.
    Choice(&'static [&'static str]),
    /// A finite number in an inclusive range.
    Number {
        /// Inclusive lower bound.
        min: f64,
        /// Inclusive upper bound.
        max: f64,
    },
    /// A number in an inclusive range, where 0 is a separate "dynamic default" sentinel.
    ///
    /// The schema still publishes the real range (e.g. 60..=3600 s); only the sentinel is extra.
    NumberOrZero {
        /// Inclusive lower bound for non-zero values.
        min: f64,
        /// Inclusive upper bound.
        max: f64,
    },
    /// Free text (up to [`TEXT_MAX`] characters, no control characters).
    Text,
}

impl SettingKind {
    /// `(kind, options, min, max)` for the schema DTO.
    pub(crate) fn describe(&self) -> (&'static str, Vec<&'static str>, Option<f64>, Option<f64>) {
        match self {
            Self::Choice(options) => ("choice", options.to_vec(), None, None),
            Self::Number { min, max } | Self::NumberOrZero { min, max } => {
                ("number", Vec::new(), Some(*min), Some(*max))
            }
            Self::Text => ("text", Vec::new(), None, None),
        }
    }

    /// Render the stored text as the typed value; `None` when it does not parse.
    pub(crate) fn parse(&self, raw: &str) -> Option<Value> {
        match self {
            Self::Choice(options) => options
                .contains(&raw)
                .then(|| Value::String(raw.to_owned())),
            Self::Number { min, max } => raw
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite() && *value >= *min && *value <= *max)
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number),
            Self::NumberOrZero { min, max } => raw
                .parse::<f64>()
                .ok()
                .filter(|value| {
                    value.is_finite() && (*value == 0.0 || (*value >= *min && *value <= *max))
                })
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number),
            Self::Text => Some(Value::String(raw.to_owned())),
        }
    }

    /// Validate the text form; the error is a sentence for the API message.
    pub(crate) fn validate(&self, raw: &str) -> Result<(), String> {
        match self {
            Self::Choice(options) => {
                if options.contains(&raw) {
                    Ok(())
                } else {
                    Err(format!(
                        "value {raw:?} is not allowed; expected one of: {}",
                        options.join(", ")
                    ))
                }
            }
            Self::Number { min, max } | Self::NumberOrZero { min, max } => {
                let Ok(value) = raw.parse::<f64>() else {
                    return Err(format!("value {raw:?} is not a number"));
                };
                if value.is_finite()
                    && (value >= *min && value <= *max
                        || matches!(self, Self::NumberOrZero { .. }) && value == 0.0)
                {
                    Ok(())
                } else {
                    Err(format!(
                        "value {raw} is outside the allowed range {min}..={max}"
                    ))
                }
            }
            Self::Text => {
                if raw.len() > TEXT_MAX {
                    return Err(format!("value is longer than {TEXT_MAX} characters"));
                }
                if raw.chars().any(char::is_control) {
                    return Err("value contains a control character".to_owned());
                }
                Ok(())
            }
        }
    }

    /// The JSON type name, for error details.
    #[allow(dead_code)]
    pub(crate) fn json_kind(&self) -> Value {
        match self {
            Self::Choice(_) | Self::Text => json!("string"),
            Self::Number { .. } | Self::NumberOrZero { .. } => json!("number"),
        }
    }
}

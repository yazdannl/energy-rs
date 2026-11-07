//! The type of a writable setting: how it is validated and rendered.

use serde_json::{Value, json};

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
    /// Free text (up to 128 characters, no control characters).
    Text,
}

impl SettingKind {
    /// `(kind, options, min, max)` for the schema DTO.
    pub(crate) fn describe(&self) -> (&'static str, Vec<&'static str>, Option<f64>, Option<f64>) {
        match self {
            Self::Choice(options) => ("choice", options.to_vec(), None, None),
            Self::Number { min, max } => ("number", Vec::new(), Some(*min), Some(*max)),
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
            Self::Number { min, max } => {
                let Ok(value) = raw.parse::<f64>() else {
                    return Err(format!("value {raw:?} is not a number"));
                };
                if value.is_finite() && value >= *min && value <= *max {
                    Ok(())
                } else {
                    Err(format!(
                        "value {raw} is outside the allowed range {min}..={max}"
                    ))
                }
            }
            Self::Text => {
                if raw.len() > 128 {
                    return Err("value is longer than 128 characters".to_owned());
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
            Self::Number { .. } => json!("number"),
        }
    }
}

//! Which PV forecast drives the plan.

/// Where the PV series comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PvMode {
    /// The learned model when it is configured and has weather; otherwise the provider, then the
    /// measured fallback. The shipped choice.
    #[default]
    Auto,
    /// The learned model only: no provider, no EWMA (an unusable model refuses the plan).
    Model,
    /// Home Assistant's `energy/solar_forecast` provider only (the pre-0.10 behaviour).
    Provider,
}

impl PvMode {
    /// Parse the setting (`auto`, `model`, `provider`); anything else is `auto`.
    pub fn parse(text: &str) -> Self {
        match text.trim().to_lowercase().as_str() {
            "model" => Self::Model,
            "provider" => Self::Provider,
            _ => Self::Auto,
        }
    }

    /// The configured string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Model => "model",
            Self::Provider => "provider",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PvMode;

    #[test]
    fn the_setting_parses_and_round_trips() {
        for mode in [PvMode::Auto, PvMode::Model, PvMode::Provider] {
            assert_eq!(PvMode::parse(mode.as_str()), mode);
        }
        assert_eq!(PvMode::parse(" Model "), PvMode::Model);
        assert_eq!(PvMode::parse("nonsense"), PvMode::Auto);
    }
}

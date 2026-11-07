//! Input provenance changes that make a stored plan outdated.

/// Which upstream input changed since the plan was solved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changes {
    /// A price slot was republished or the price feed changed.
    pub prices: bool,
    /// The PV or load forecast was refreshed.
    pub forecast: bool,
}

impl Changes {
    /// Both inputs unchanged.
    pub const fn none() -> Self {
        Self {
            prices: false,
            forecast: false,
        }
    }
}

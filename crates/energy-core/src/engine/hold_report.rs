//! The three-state result of one hold pass.

/// Which packs were verified, left unverified or fenced out of a hold pass.
///
/// Only `verified` packs may be counted as held: a missing echo or a timeout leaves the pack latched,
/// and a fenced command never reached the device. Callers that need a human summary use
/// [`HoldReport::summary`], which names the packs in each non-verified group.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HoldReport {
    verified: Vec<String>,
    unverified: Vec<String>,
    fenced: Vec<String>,
}

impl HoldReport {
    pub(crate) fn new(verified: Vec<String>, unverified: Vec<String>, fenced: Vec<String>) -> Self {
        Self {
            verified,
            unverified,
            fenced,
        }
    }

    /// Packs whose hold was confirmed by a fresh device echo.
    pub fn verified(&self) -> &[String] {
        &self.verified
    }

    /// Packs that were written but not confirmed, or that timed out; still latched.
    pub fn unverified(&self) -> &[String] {
        &self.unverified
    }

    /// Packs whose command was refused by the authority or generation gate; no write, still latched.
    pub fn fenced(&self) -> &[String] {
        &self.fenced
    }

    /// One summary line that names every pack that may still be driving.
    pub fn summary(&self) -> String {
        format!(
            "{} verified, {} unverified ({}), {} fenced ({})",
            self.verified.len(),
            self.unverified.len(),
            names(&self.unverified),
            self.fenced.len(),
            names(&self.fenced)
        )
    }
}

fn names(packs: &[String]) -> String {
    packs.join(", ")
}

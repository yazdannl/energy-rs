//! Typed outcome recorded per configured grid source.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceOutcome {
    /// Full coverage, summed into the numeric series.
    Ok,
    /// Optional source with missing slots; those slots contribute zero.
    OptionalZero,
    /// Required source with a gap while another source covers the category.
    PartialRequired,
    /// Complete but not selected to contribute; another source is authoritative.
    Redundant,
    /// Required category cannot be summed; the plan is refused.
    InputsUnavailable,
}

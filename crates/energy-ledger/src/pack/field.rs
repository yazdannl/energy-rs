//! One field of an operator pack edit: leave it alone, clear it, or set a value.

/// The three states of a writable pack field in one PATCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackField<T> {
    /// The key was absent: keep the stored value.
    Keep,
    /// The key was `null`: write `NULL`.
    Clear,
    /// The key carried a value: write it.
    Set(T),
}

impl<T> PackField<T> {
    /// The value when one was supplied, for read-only checks before the write.
    pub fn as_option(&self) -> Option<&T> {
        match self {
            Self::Clear | Self::Keep => None,
            Self::Set(value) => Some(value),
        }
    }
}

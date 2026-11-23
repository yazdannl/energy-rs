//! One original SQLite value, kept with its storage type and bytes.

/// A value copied out of a poisoned outbox row without decoding it.
///
/// `storage_type` is SQLite's `typeof()` tag and `bytes` is `CAST(value AS BLOB)`, so TEXT with
/// invalid UTF-8 and genuine SQL NULL are distinguishable and reconstructible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawValue {
    pub storage_type: String,
    pub bytes: Vec<u8>,
}

impl RawValue {
    /// Build a value from SQLite's type tag and blob cast.
    pub fn new(storage_type: impl Into<String>, bytes: Option<Vec<u8>>) -> Self {
        Self {
            storage_type: storage_type.into(),
            bytes: bytes.unwrap_or_default(),
        }
    }

    /// A text value copied from a decoded learning event.
    pub fn text(value: &str) -> Self {
        Self::new("text", Some(value.as_bytes().to_vec()))
    }

    /// An integer value copied from a decoded learning event.
    pub fn integer(value: i64) -> Self {
        Self::new("integer", Some(value.to_string().into_bytes()))
    }

    /// A genuine SQL NULL, distinct from the text `"null"`.
    pub fn null() -> Self {
        Self::new("null", None)
    }

    /// The original TEXT bytes, when this value really was TEXT.
    pub fn as_text(&self) -> Option<&[u8]> {
        (self.storage_type == "text").then_some(self.bytes.as_slice())
    }

    /// Whether this was a genuine SQL NULL.
    pub fn is_null(&self) -> bool {
        self.storage_type == "null"
    }

    /// A safe label for an alert message; never used as the preserved payload.
    pub fn label(&self) -> String {
        if self.is_null() {
            return "null".to_owned();
        }
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

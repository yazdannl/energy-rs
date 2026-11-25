//! Opaque authority token; the digest is an integrity check, not host-attested proof.

/// Durable latch token. Its generation text is preserved so a malformed external edit
/// is detectable instead of being repaired silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatchToken {
    generation_text: String,
    generation: Option<u64>,
    digest: String,
}

impl LatchToken {
    /// Mint a token for the sealed bootstrap transaction. Commit still requires provenance,
    /// matching event and an unused single-use nonce.
    pub fn mint(generation: u64, seed: &str) -> Self {
        let generation_text = generation.to_string();
        let digest = crate::authority::journal::hash_text(&format!("{seed}:{generation_text}"));
        Self {
            generation: Some(generation),
            generation_text,
            digest,
        }
    }

    /// Reconstruct from storage exactly as written; malformed generations stay invalid.
    pub fn from_storage(generation_text: &str, seed: &str) -> Self {
        let generation = generation_text.parse::<u64>().ok();
        let digest = crate::authority::journal::hash_text(&format!("{seed}:{generation_text}"));
        Self {
            generation,
            generation_text: generation_text.to_owned(),
            digest,
        }
    }

    /// Parsed generation; absence means malformed and refuses authority.
    pub fn generation(&self) -> Option<u64> {
        self.generation
    }
    /// Exact generation text found in storage.
    pub fn generation_text(&self) -> &str {
        &self.generation_text
    }
    /// Integrity digest stored in the separate anchor.
    pub fn hash(&self) -> &str {
        &self.digest
    }
}

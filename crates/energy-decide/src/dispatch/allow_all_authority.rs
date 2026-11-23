//! Default authority port for tests and unwired modules.

use crate::dispatch::{AuthorityDecision, PackCommand, WriteAuthority};

/// Authority gate that allows every command; real M6 replaces it.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAllAuthority;

impl WriteAuthority for AllowAllAuthority {
    fn authorize(&self, _command: &PackCommand) -> AuthorityDecision {
        AuthorityDecision::Authorized
    }
}

//! Resolve a pack reference (serial, alias or name) against the ledger.

use energy_ledger::Pack;

use crate::api::blocking::with_ledger;
use crate::api::error::ApiError;
use crate::service::Service;

/// Look one pack up by serial number, operator alias or display name.
pub(crate) async fn resolve_pack(service: &Service, reference: &str) -> Result<Pack, ApiError> {
    let reference = reference.trim().to_owned();
    let wanted = reference.clone();
    let found = with_ledger(service, move |ledger| {
        ledger
            .packs()
            .map_err(|err| ApiError::internal("look up pack", err))?
            .all()
            .map_err(|err| ApiError::internal("look up pack", err))
            .map(|packs| {
                packs.into_iter().find(|pack| {
                    pack.sn == wanted
                        || pack.alias.as_deref() == Some(&wanted)
                        || pack.name.as_deref() == Some(&wanted)
                })
            })
    })
    .await?;
    found.ok_or_else(|| ApiError::unknown_pack(&reference))
}

/// Resolve an optional reference; `None` means "the whole fleet" and stays `None`.
pub(crate) async fn resolve_optional_pack(
    service: &Service,
    reference: Option<&str>,
) -> Result<Option<String>, ApiError> {
    match reference {
        None => Ok(None),
        Some(reference) => Ok(Some(resolve_pack(service, reference).await?.sn)),
    }
}

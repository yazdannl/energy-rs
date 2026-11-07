//! Proof that the loaded C artifact was built from the sources shipped in this binary.

use std::sync::OnceLock;

use crate::error::SolveError;
use crate::ffi;

/// The C sources this binary was built from, as `build.rs` framed them.
fn source_digest() -> String {
    crate::sha256::digest_framed(&[
        (
            "csrc/core_solver.h",
            include_bytes!("../../csrc/core_solver.h"),
        ),
        (
            "csrc/core_solver.c",
            include_bytes!("../../csrc/core_solver.c"),
        ),
        (
            "csrc/core_adapter.c",
            include_bytes!("../../csrc/core_adapter.c"),
        ),
        (
            "csrc/energy_opt_ffi.c",
            include_bytes!("../../csrc/energy_opt_ffi.c"),
        ),
    ])
}

/// The digest the C artifact reports (`-DCORE_BUILD_SHA`), or `""` when it was not compiled in.
pub fn core_build_sha() -> String {
    ffi::build_sha()
}

/// Prove the loaded C artifact was built from the sources shipped in this binary.
///
/// This is the port of `_fast.stale_reason`: the digest covers the sources, so an object file that
/// was cached, copied or restored from a different revision cannot pass as a build of the current
/// source.  Checked once per process; every [`super::solve`] calls it.
pub fn verify_core_build() -> Result<(), SolveError> {
    static VERIFIED: OnceLock<Result<(), SolveError>> = OnceLock::new();
    VERIFIED
        .get_or_init(|| {
            let expected = env!("EO_CORE_SOURCE_SHA256");
            let shipped = source_digest();
            if shipped != expected {
                return Err(SolveError::StaleCore {
                    built: expected.chars().take(16).collect(),
                    expected: shipped.chars().take(16).collect(),
                });
            }
            let built = ffi::build_sha();
            let wanted = &expected[..16];
            if built != wanted {
                return Err(SolveError::StaleCore {
                    built,
                    expected: wanted.to_string(),
                });
            }
            Ok(())
        })
        .clone()
}

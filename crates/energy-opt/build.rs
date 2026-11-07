//! Build script for the DP core.
//!
//! P1 will vendor the old add-on's `_core.c` byte-identically and compile it here with `cc` using
//! `-O3 -ffp-contract=off` (bit-reproducibility matters; the plan is compared bit-for-bit against
//! the Rust oracle). The C core is deliberately **not** vendored in P0, so this script is a
//! no-op for now — see `README.md`.
fn main() {
    // TODO(P1): compile `src/core.c` through the `cc` crate with the exact old flags and expose
    // its version via `cargo:rustc-env`. Do not add `rerun-if-changed` for a file that does not
    // exist yet: cargo warns about it.
    println!("cargo:rerun-if-changed=build.rs");
}

//! Build script for the DP core.
//!
//! Compiles the plain C ABI (`csrc/energy_opt_ffi.c` + the bridge `csrc/core_adapter.c`, which
//! includes the extracted `csrc/core_solver.c`) with the old add-on's flags:
//!
//! ```text
//! gcc -O3 -ffp-contract=off -fno-fast-math -DCORE_BUILD_SHA="<sha16>"
//! ```
//!
//! * `-O3` is the shipped optimisation level.
//! * `-ffp-contract=off` is mandatory, not stylistic: FMA contraction flips near-tie argmins and
//!   breaks the differential against the Rust oracle.
//! * `-fno-fast-math` keeps every expression the IEEE operation the reference performs.
//! * `-DCORE_BUILD_SHA` embeds the digest of the C sources into the artifact, which the crate
//!   re-checks at runtime against the sources it shipped (`verify_core`), so a stale object file
//!   cannot run.
//!
//! `-Wno-unused-function` is belt and braces for the vendored text: the extracted file still
//! carries the helpers only the CPython entry point used.

use std::path::{Path, PathBuf};

#[path = "src/sha256.rs"]
mod sha256;

/// The C sources the artifact is built from, in the order the digest frames them.
const SOURCES: [&str; 4] = [
    "csrc/core_solver.h",
    "csrc/core_solver.c",
    "csrc/core_adapter.c",
    "csrc/energy_opt_ffi.c",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let mut files: Vec<(&str, Vec<u8>)> = Vec::with_capacity(SOURCES.len());
    for name in SOURCES {
        let path = manifest.join(name);
        println!("cargo:rerun-if-changed={name}");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        files.push((name, bytes));
    }
    let framed: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(name, bytes)| (*name, bytes.as_slice()))
        .collect();
    let full_sha = sha256::digest_framed(&framed);
    let build_sha = &full_sha[..16];

    let mut build = cc::Build::new();
    for source in ["csrc/core_adapter.c", "csrc/energy_opt_ffi.c"] {
        build.file(source);
    }
    build
        .include(manifest.join("csrc"))
        .include(Path::new("csrc"))
        .opt_level(3)
        .flag_if_supported("-std=c11")
        .flag("-ffp-contract=off")
        .flag("-fno-fast-math")
        .flag_if_supported("-Wno-unused-function")
        .define("CORE_BUILD_SHA", format!("\"{build_sha}\"").as_str())
        .compile("energy_opt_core");

    println!("cargo:rustc-env=EO_CORE_BUILD_SHA={build_sha}");
    println!("cargo:rustc-env=EO_CORE_SOURCE_SHA256={full_sha}");
}

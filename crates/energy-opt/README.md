# energy-opt

DP optimizer core (PLAN.md §2, §6).

P0 scaffold: modules and docs only.

## Not yet vendored: the C core

The old add-on's `_core.c` is **not** copied into this repository in P0. P1 vendors it
byte-identically as `crates/energy-opt/src/core.c` and switches `build.rs` from a no-op to a `cc`
build with the old flags (`-O3 -ffp-contract=off`), because bit-for-bit reproducibility of the DP
is one of the few hard requirements of the rewrite.

Until then `build.rs` only emits `cargo:rerun-if-changed=build.rs` and `dp`/`oracle`/`validate`
are documentation-only modules.

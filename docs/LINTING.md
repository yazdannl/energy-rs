# Linting, formatting and structural limits

The rules below are enforced by CI (`.github/workflows/ci.yml`) and are not advisory. A branch
that fails any of them is not merged, however good the code is: an optimizer that plans battery
writes must not have a reachable panic path, and a file nobody can review is a defect of its own.

```
cargo fmt --all -- --check                    # fmt job
cargo clippy --all-targets -- -D warnings     # clippy job
python3 scripts/check_structure.py            # structure job
cargo test --workspace                        # test job
cd web && npm ci && npm run lint              # web-lint (also npm run format:check)
cd web && npm run check                       # web-check  (svelte-check)
cd web && npm run build                       # web-build
cd web && npm test                            # web-test  (vitest; also re-runs the four above)
```

## Rust

### Workspace policy

`Cargo.toml` carries the policy in `[workspace.lints]`; every crate opts in with

```toml
[lints]
workspace = true
```

so a new crate cannot silently escape it.

| Group / lint | Level | Why |
|---|---|---|
| `clippy::all` | deny | the standard set |
| `clippy::pedantic` | deny | this is numeric control code; the extra lints catch real mistakes |
| `clippy::unwrap_used` | deny | a panic in a planning tick is an outage |
| `clippy::expect_used` | deny | same |
| `clippy::panic` | deny | same |
| `clippy::todo` | deny | unfinished code does not ship |
| `clippy::too_many_arguments` | deny | threshold 6 (`clippy.toml`) |
| `clippy::too_many_lines` | deny | threshold 60 (`clippy.toml`) |
| `clippy::cognitive_complexity` | deny | threshold 15 (`clippy.toml`) |
| `clippy::large_enum_variant` | deny | typed errors must stay cheap to move |
| `rustc::unused_must_use` | deny | a dropped `Result` is a dropped error |
| `rustc::unsafe_op_in_unsafe_fn` | deny | the FFI shim keeps its `unsafe` blocks explicit |

### Allowed pedantic lints (workspace level)

Each of these is a deliberate, documented exception rather than an accident. They are group-level
style preferences that would otherwise generate noise without changing behaviour:

| Lint | Reason |
|---|---|
| `module_name_repetitions` | domain names read correctly with their module prefix (`client::Client`, `validate::Slot`) |
| `must_use_candidate` | public getters are consumed immediately; a `#[must_use]` on every one is noise |
| `missing_errors_doc` | errors are typed with `thiserror` and described in the module docs |
| `missing_panics_doc` | there are no panics to document (see the denials above) |
| `cast_precision_loss` | `usize`/`i64` → `f64` is the domain: slot counts, indices, iterations |
| `cast_possible_truncation` | `f64` → `i64` truncation is explicit in the grid math, not accidental |
| `similar_names` | numeric code reuses shapes on purpose (`e_now`/`e_next`, `lo`/`hi`); renaming hurts review |
| `doc_markdown` | doc comments quote C sources and Python identifiers that are not Rust items |

### Formatting

`rustfmt.toml` sets `max_width = 100` (edition 2024). `cargo fmt --all` before committing;
CI runs the check, so a formatting-only diff is never part of a review.

## Structural limits (`scripts/check_structure.py`)

Clippy cannot see files or modules as a whole, so the checker covers what it cannot:

| Rule | Limit | Scope |
|---|---|---|
| primary public types per file | 1 (`pub struct` / `pub enum` / `pub trait`) | `lib.rs`, `mod.rs`, `build.rs` are wiring and exempt |
| file length | 300 lines | `crates/*/src/**/*.rs` |
| file length | 500 lines | anything under a `tests/` directory |
| function length | 60 lines (body) | every `.rs` file |
| function parameters | 6 (`self` does not count) | every `.rs` file |
| catch-all modules | forbidden | `utils`, `helpers`, `misc`, `common` as a file name or a directory |
| Svelte component length | 250 lines | `web/src/**/*.svelte` |
| Svelte component name | file name equals the component, `PascalCase.svelte` | `web/src/**/*.svelte` |

Exempt: `csrc/` (vendored C, byte for byte from upstream), `target/`, `node_modules/`, `web/dist/`.

The checker prints one row per violation and exits 1:

```
file                                   line  rule            detail
------------------------------------  -----  --------------  ----------------------------------------
crates/energy-opt/src/oracle.rs        494  file-length     494 lines > 300
```

Split by behaviour, not by line count: `validate/` holds one file per aspect (arithmetic, windows,
grid, verdict, path, backward), `oracle/` one per stage (grid, sweep, replay). A file that needs
splitting is a file that has more than one job.

## Web

ESLint 9 flat config (`web/eslint.config.js`): `@eslint/js` recommended, `typescript-eslint`
recommended, `eslint-plugin-svelte` recommended, and `@typescript-eslint/no-explicit-any` as an
**error** — an implicit `any` at the REST boundary is exactly the bug this UI cannot afford.

Prettier (`.prettierrc.json`) formats `.ts`, `.svelte`, `.css`, `.json` and `.md` with
`prettier-plugin-svelte`; `printWidth` 100 matches rustfmt. `npm run check` runs `svelte-check`
with the repo `tsconfig.json` (strict, `noEmit`).

## Justifying an exception

An exception is a comment, never a silent edit.

1. **Prefer moving the code.** Most violations are a file with two jobs or a function that wants a
   context struct. Do that first.
2. **Local `allow`, with the reason on the item.** Never at crate root (`#![allow(...)]` on a
   production `lib.rs`/`main.rs` is rejected in review). For example:

   ```rust
   // A decision is a discrete choice: the reference argmin must be reproduced exactly.
   #[allow(clippy::float_cmp)]
   fn slot(...) {
   ```

   The reason must say *why the lint is wrong here*, not what the lint is.
3. **Test-only relaxations live in the test file.** Integration tests start with a documented
   header:

   ```rust
   // Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
   #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
   ```

   `#[cfg(test)]` modules inside `src/` put the same attribute at the top of the module.
4. **Build scripts may panic.** `crates/energy-opt/build.rs` allows `unwrap_used`/`expect_used`/
   `panic` with that reason: failing loudly is the only way a build script can report a problem.
5. **Numeric fidelity beats style.** Where a lint would change an arithmetic operation the C core
   performs (for example `manual_midpoint`: `(a + b) / 2` and `f64::midpoint` round differently),
   keep the reference expression and allow the lint with that reason.
6. **Workspace-level allows** are the last resort: they need an entry in the table above and a
   review of every crate they now cover.

Current deliberate exceptions (grep for `allow(clippy::` to list them):

| Where | Lint | Reason |
|---|---|---|
| `energy-opt/build.rs` | `unwrap_used`, `expect_used`, `panic` | a build script fails by panicking |
| `energy-opt/src/sha256.rs` | `many_single_char_names` | FIPS 180-4 names its working variables `a`..`h`, `w` |
| `energy-opt/src/ffi/owned.rs` | `unused_self` | the `&self` receiver is the lifetime tie for the C arena |
| `energy-opt/src/validate/{window,verdict,path,backward}.rs` | `manual_midpoint`, `float_cmp`, `cast_sign_loss` | bit-exact replay of the C core, or a decision/index that is exact by definition |
| `energy-ha/src/stats/history_query.rs` | `struct_excessive_bools` | the four booleans are the REST body's fields |
| `tests/**` | `unwrap_used`, `expect_used`, `panic`, `assert_is_empty`, `float_cmp`, `manual_midpoint`, `unreadable_literal` | test code reports by failing; fixture epochs and money read better verbatim |

#!/usr/bin/env python3
"""Extract the pure-C numeric section of the old add-on's CPython extension (P1).

Source of truth: ``ha-energy-manager/energy_manager/app/optimize/_core.c`` (read-only).  Only the
part that touches no Python object is vendored: the ``Config`` struct, the helpers and
``run_solve``.  The surrounding CPython glue (config parsing, sequence conversion, the result
dict) is *not* extracted -- the port replaces it with a flat C ABI (``csrc/energy_opt_ffi.c``) and
a Rust API.

The extraction is a line-range copy, so it is reproducible and auditable::

    python3 scripts/extract_core.py            # (re)generate csrc/core_solver.c
    python3 scripts/extract_core.py --check    # fail when the checked-in file is stale

The upstream digest is pinned below; a different ``_core.c`` is refused rather than silently
vendored, because every value in the solver depends on those exact expression orders.  The SHA
that ends up in ``-DCORE_BUILD_SHA=<sha16>`` is computed by ``build.rs`` from the *generated*
file, and the generated file is byte-checked against these ranges on every run of this script.
"""

from __future__ import annotations

import argparse
import hashlib
import pathlib
import sys

#: ``sha256`` of the upstream ``_core.c`` this extraction is pinned to (0.38.8).
UPSTREAM_SHA256 = "019c1d04194f45e7c527db86532a1f17bfaec7c35226e2eb1586e70e67125894"

#: Repo-relative location of the upstream file (the old add-on; never modified by this project).
UPSTREAM_RELPATH = "ha-energy-manager/energy_manager/app/optimize/_core.c"

#: 1-based, inclusive line ranges of the numeric section, in file order.
#:
#: 43-50   the ``CORE_BUILD_SHA`` fallback the artifact identity relies on
#: 52-98   the dp.py-mirroring constants, the dimension caps and the ``Config`` struct
#: 100-535 ``SlotCtx``, every arithmetic helper and ``run_solve`` (sweep + reconstruction)
#: 660-708 ``validate_slots``, the per-slot domain guard the CPython entry point ran
RANGES: tuple[tuple[int, int], ...] = ((43, 50), (52, 98), (100, 535), (660, 708))

BANNER = """\
/* GENERATED FILE - DO NOT EDIT.
 *
 * The numeric core of the old add-on's CPython extension, extracted verbatim from
 *   {upstream}
 * (sha256 {sha256}), lines {ranges}.
 *
 * Regenerate with `python3 scripts/extract_core.py`; `--check` proves this file is current.
 * Everything below the include is byte-identical to the upstream ranges -- including the
 * `PyErr_SetString`/`PyErr_Format` call sites, which are mapped onto a local shim in
 * core_solver.h so the extracted text needs no editing.  The CPython entry point, the dict and
 * sequence marshalling and the config parser are deliberately NOT vendored:
 * csrc/energy_opt_ffi.c implements the C ABI.
 *
 * See csrc/UPSTREAM.md for what is extracted, what the shim covers and why.
 */
#include "core_solver.h"

"""


def repo_root() -> pathlib.Path:
    return pathlib.Path(__file__).resolve().parent.parent


def upstream_path(explicit: str | None) -> pathlib.Path:
    if explicit:
        return pathlib.Path(explicit).expanduser().resolve()
    root = repo_root()
    candidate = root.parent / UPSTREAM_RELPATH
    if candidate.exists():
        return candidate
    return pathlib.Path("/root/HomeAssitant") / UPSTREAM_RELPATH


def read_upstream(path: pathlib.Path) -> tuple[str, list[str]]:
    if not path.exists():
        raise SystemExit(f"extract_core: upstream not found: {path}")
    raw = path.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != UPSTREAM_SHA256:
        raise SystemExit(
            "extract_core: upstream digest mismatch\n"
            f"  expected {UPSTREAM_SHA256}\n"
            f"  actual   {digest}\n"
            f"  file     {path}\n"
            "Pin the new source deliberately (and re-run the differential) instead of vendoring it "
            "blindly."
        )
    return digest, raw.decode("utf-8").splitlines(keepends=True)


def extract(lines: list[str]) -> str:
    chunks = []
    for start, end in RANGES:
        if end > len(lines):
            raise SystemExit(f"extract_core: range {start}-{end} is past the end of the file")
        chunks.append("".join(lines[start - 1 : end]))
    return "".join(chunks)


def render(lines: list[str]) -> tuple[str, str]:
    body = extract(lines)
    ranges = ", ".join(f"{start}-{end}" for start, end in RANGES)
    banner = BANNER.format(upstream=UPSTREAM_RELPATH, sha256=UPSTREAM_SHA256, ranges=ranges)
    return banner, body


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--upstream", help="path to the old add-on's _core.c")
    parser.add_argument(
        "--out",
        default=str(repo_root() / "crates" / "energy-opt" / "csrc" / "core_solver.c"),
        help="generated file (default: crates/energy-opt/csrc/core_solver.c)",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify the generated file is current instead of writing it",
    )
    parser.add_argument("--print-ranges", action="store_true", help="list the extracted ranges")
    args = parser.parse_args(argv)

    path = upstream_path(args.upstream)
    digest, lines = read_upstream(path)
    banner, body = render(lines)

    if args.print_ranges:
        for start, end in RANGES:
            first = lines[start - 1].rstrip()
            last = lines[end - 1].rstrip()
            print(f"{start:4d}-{end:4d}  {first[:60]!r} .. {last[:60]!r}")

    if args.check:
        target = pathlib.Path(args.out)
        if not target.exists():
            print(f"extract_core: {target} is missing", file=sys.stderr)
            return 1
        current = target.read_text(encoding="utf-8")
        expected = banner + body
        if current != expected:
            print(f"extract_core: {target} is stale; run scripts/extract_core.py", file=sys.stderr)
            return 1
        print(f"extract_core: {target} matches {path.name} {digest[:16]}")
        return 0

    target = pathlib.Path(args.out)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(banner + body, encoding="utf-8")

    # Round-trip proof: the payload in the written file must equal the upstream ranges byte for
    # byte, so a range boundary that cuts a line is a failure, not a subtly different build.
    written = target.read_text(encoding="utf-8")
    if not written.startswith(banner) or written[len(banner) :] != extract(lines):
        print("extract_core: round-trip check failed", file=sys.stderr)
        return 1
    print(f"extract_core: wrote {target} ({len(body.splitlines())} lines from {path.name})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

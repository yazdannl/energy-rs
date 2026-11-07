#!/usr/bin/env python3
"""Enforce the structural limits from docs/LINTING.md.

Stdlib only.  Exits 1 when anything is violated and prints one table row per violation, so a CI
failure names the file, the line and the rule instead of a wall of text.

Rules
-----
* one primary `pub struct` / `pub enum` / `pub trait` per `.rs` file (`lib.rs`, `mod.rs` and
  `build.rs` are wiring, not a home for a type; `#[cfg(test)]` modules are ignored)
* `.rs` files: 300 lines, 500 under `tests/`
* functions: 60 lines, 6 parameters (`self` does not count)
* no `utils.rs` / `helpers.rs` / `misc.rs` / `common.rs` anywhere
* Svelte components: 250 lines, file name is the component name (`PascalCase.svelte`)
* `csrc/` (vendored C), `target/`, `node_modules/` and `web/dist/` are exempt
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

MAX_SRC_LINES = 300
MAX_TEST_LINES = 500
MAX_FN_LINES = 60
MAX_PARAMS = 6
MAX_SVELTE_LINES = 250
MAX_PUB_TYPES = 1

FORBIDDEN_STEMS = {"utils", "helpers", "misc", "common"}
SKIP_DIRS = {"target", "node_modules", "dist", ".git", "csrc"}
TYPE_KEYWORDS = ("struct", "enum", "trait")
COMPONENT_NAME = re.compile(r"^[A-Z][A-Za-z0-9]*$")


@dataclass(frozen=True)
class Violation:
    path: str
    line: int
    rule: str
    detail: str

    def sort_key(self) -> tuple[str, int, str]:
        return (self.path, self.line, self.rule)


def blank_non_code(source: str) -> str:
    """Replace comments, strings and char literals with spaces, keeping every newline.

    Positions and line numbers in the result match the original, so the structural scanners can
    work on clean text while errors still point at the real file.
    """
    out: list[str] = []
    i = 0
    n = len(source)
    while i < n:
        ch = source[i]
        two = source[i : i + 2]
        if two == "//":
            end = source.find("\n", i)
            end = n if end < 0 else end
            out.append(" " * (end - i))
            i = end
        elif two == "/*":
            depth = 1
            j = i + 2
            while j < n and depth:
                if source[j : j + 2] == "/*":
                    depth += 1
                    j += 2
                elif source[j : j + 2] == "*/":
                    depth -= 1
                    j += 2
                else:
                    j += 1
            out.append("".join("\n" if c == "\n" else " " for c in source[i:j]))
            i = j
        elif ch == '"' or (ch in "rb" and re.match(r'b?r#*"', source[i : i + 12])):
            j = _string_end(source, i)
            out.append("".join("\n" if c == "\n" else " " for c in source[i:j]))
            i = j
        elif ch == "'":
            j = _char_or_lifetime_end(source, i)
            out.append("".join("\n" if c == "\n" else " " for c in source[i:j]))
            i = j
        else:
            out.append(ch)
            i += 1
    return "".join(out)


def _string_end(source: str, start: int) -> int:
    """End index (exclusive) of the string literal starting at `start`."""
    i = start
    if source.startswith("br", i) or source.startswith("rb", i):
        i += 2
    elif source[i] in "rb":
        i += 1
    hashes = 0
    while i < len(source) and source[i] == "#":
        hashes += 1
        i += 1
    if i >= len(source) or source[i] != '"':
        return start + 1
    i += 1
    closing = '"' + "#" * hashes
    while i < len(source):
        if source[i] == "\\" and hashes == 0:
            i += 2
            continue
        if source.startswith(closing, i):
            return i + len(closing)
        i += 1
    return len(source)


def _char_or_lifetime_end(source: str, start: int) -> int:
    """End index of a char literal, or of a lifetime/label when this is not a char."""
    if re.match(r"'(?:\\.|[^\\'\n])'", source[start:]):
        match = re.match(r"'(?:\\.|[^\\'\n])'", source[start:])
        return start + len(match.group(0))
    match = re.match(r"'[A-Za-z_][A-Za-z0-9_]*", source[start:])
    return start + (len(match.group(0)) if match else 1)


def match_brace(masked: str, start: int) -> int:
    """Index of the `}` matching the `{` at `start` (or `len(masked)`)."""
    depth = 0
    for i in range(start, len(masked)):
        if masked[i] == "{":
            depth += 1
        elif masked[i] == "}":
            depth -= 1
            if depth == 0:
                return i
    return len(masked)


def match_paren(masked: str, start: int) -> int:
    depth = 0
    for i in range(start, len(masked)):
        if masked[i] == "(":
            depth += 1
        elif masked[i] == ")":
            depth -= 1
            if depth == 0:
                return i
    return len(masked)


def line_of(text: str, index: int) -> int:
    return text.count("\n", 0, index) + 1


def cfg_test_ranges(masked: str) -> list[tuple[int, int]]:
    """Character ranges covered by `#[cfg(test)] mod .. { .. }` blocks."""
    ranges = []
    for match in re.finditer(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]", masked):
        brace = masked.find("{", match.end())
        if brace < 0:
            continue
        ranges.append((match.start(), match_brace(masked, brace)))
    return ranges


def inside(ranges: list[tuple[int, int]], index: int) -> bool:
    return any(start <= index <= end for start, end in ranges)


def pub_types(masked: str, test_ranges: list[tuple[int, int]]) -> list[tuple[int, str]]:
    found = []
    pattern = re.compile(r"\bpub\s+(?:struct|enum|trait)\s+([A-Za-z_][A-Za-z0-9_]*)")
    for match in pattern.finditer(masked):
        if inside(test_ranges, match.start()):
            continue
        found.append((line_of(masked, match.start()), match.group(1)))
    return found


def function_spans(masked: str) -> list[tuple[str, int, int, int, int]]:
    """`(name, fn_index, line, body_start_line, body_end_line)` for every `fn` with a body."""
    spans = []
    for match in re.finditer(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)", masked):
        name = match.group(1)
        paren = masked.find("(", match.end())
        if paren < 0:
            continue
        close = match_paren(masked, paren)
        body = close + 1
        while body < len(masked) and masked[body] in " \t\r\n":
            body += 1
        # Walk an optional return type / where clause to the body brace or the declaration `;`.
        while body < len(masked) and masked[body] not in "{;":
            if masked[body] in "([":
                body = match_paren(masked, body) + 1
                continue
            body += 1
        if body >= len(masked) or masked[body] == ";":
            continue
        end = match_brace(masked, body)
        spans.append(
            (
                name,
                match.start(),
                line_of(masked, match.start()),
                line_of(masked, body),
                line_of(masked, end),
            )
        )
    return spans


def param_count(masked: str, fn_index: int) -> int:
    paren = masked.find("(", fn_index)
    if paren < 0:
        return 0
    close = match_paren(masked, paren)
    inner = masked[paren + 1 : close]
    params: list[str] = []
    depth = 0
    current: list[str] = []
    for ch in inner:
        if ch in "([<{":
            depth += 1
        elif ch in ")]>}":
            depth -= 1
        if ch == "," and depth == 0:
            params.append("".join(current))
            current = []
        else:
            current.append(ch)
    if "".join(current).strip():
        params.append("".join(current))
    params = [p.strip() for p in params if p.strip()]
    if params and re.match(
        r"(&\s*(?:'[A-Za-z_][A-Za-z0-9_]*\s*)?(?:mut\s+)?self|mut\s+self|self)", params[0]
    ):
        params = params[1:]
    return len(params)


def check_rust_file(path: Path, rel: str) -> list[Violation]:
    violations: list[Violation] = []
    source = path.read_text(encoding="utf-8", errors="replace")
    masked = blank_non_code(source)
    line_count = source.count("\n") + (0 if source.endswith("\n") else 1)
    is_test = "tests" in path.parts
    limit = MAX_TEST_LINES if is_test else MAX_SRC_LINES
    if line_count > limit:
        violations.append(Violation(rel, line_count, "file-length", f"{line_count} lines > {limit}"))

    test_ranges = cfg_test_ranges(masked)
    types = pub_types(masked, test_ranges)
    if len(types) > MAX_PUB_TYPES:
        names = ", ".join(name for _, name in types)
        for line, name in types[1:]:
            violations.append(
                Violation(rel, line, "pub-types", f"{len(types)} public types ({names}); {name} is extra")
            )

    for name, fn_index, line, body_start, body_end in function_spans(masked):
        span = body_end - body_start + 1
        if span > MAX_FN_LINES:
            violations.append(
                Violation(rel, line, "fn-length", f"fn {name}: {span} lines > {MAX_FN_LINES}")
            )
        count = param_count(masked, fn_index)
        if count > MAX_PARAMS:
            violations.append(
                Violation(rel, line, "fn-params", f"fn {name}: {count} params > {MAX_PARAMS}")
            )
    return violations


def check_svelte_file(path: Path, rel: str) -> list[Violation]:
    violations: list[Violation] = []
    source = path.read_text(encoding="utf-8", errors="replace")
    line_count = source.count("\n") + (0 if source.endswith("\n") else 1)
    if line_count > MAX_SVELTE_LINES:
        violations.append(
            Violation(rel, line_count, "svelte-length", f"{line_count} lines > {MAX_SVELTE_LINES}")
        )
    if not COMPONENT_NAME.match(path.stem):
        violations.append(
            Violation(rel, 1, "svelte-name", f"component file must be PascalCase, got {path.name}")
        )
    return violations


def walk() -> list[Violation]:
    violations: list[Violation] = []
    for path in sorted(ROOT.rglob("*")):
        if not path.is_dir() or SKIP_DIRS & set(path.relative_to(ROOT).parts):
            continue
        if path.name in FORBIDDEN_STEMS:
            violations.append(
                Violation(
                    path.relative_to(ROOT).as_posix(),
                    1,
                    "forbidden-dir",
                    f"'{path.name}/' is a catch-all module name",
                )
            )
    for path in sorted(ROOT.rglob("*")):
        if not path.is_file():
            continue
        if SKIP_DIRS & set(path.relative_to(ROOT).parts):
            continue
        rel = path.relative_to(ROOT).as_posix()
        if path.suffix == ".rs" and path.name not in {"lib.rs", "mod.rs", "build.rs"}:
            if path.stem in FORBIDDEN_STEMS:
                violations.append(
                    Violation(rel, 1, "forbidden-file", f"'{path.name}' is a catch-all module name")
                )
        if path.suffix == ".rs":
            violations.extend(check_rust_file(path, rel))
        elif path.suffix == ".svelte":
            violations.extend(check_svelte_file(path, rel))
    return sorted(violations, key=Violation.sort_key)


def main() -> int:
    violations = walk()
    if not violations:
        print(
            "structure: ok (pub types <= 1/file, src <= 300, tests <= 500, "
            f"fn <= {MAX_FN_LINES}/{MAX_PARAMS}, svelte <= {MAX_SVELTE_LINES})"
        )
        return 0
    width = max(len(v.path) for v in violations)
    print(f"{'file'.ljust(width)}  {'line':>5}  {'rule':<14}  detail")
    print(f"{'-' * width}  {'-' * 5}  {'-' * 14}  {'-' * 40}")
    for v in violations:
        print(f"{v.path.ljust(width)}  {v.line:>5}  {v.rule:<14}  {v.detail}")
    print(f"\nstructure: {len(violations)} violation(s)")
    return 1


if __name__ == "__main__":
    sys.exit(main())

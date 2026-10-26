#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Refuse a physical constant's literal value outside `nec_model::constants`.

fnec's speed of light, μ0, ε0 and η0 were defined in fourteen modules across four
crates and inlined at eight more sites. The C0/MU0/ETA0 copies happened to be
bit-identical; the two EPS0 definitions
were not (8.854 187 817e-12 in the Sommerfeld ground, 8.854 187 812 8e-12 in the
MPIE), and a third, inline in the NEC-2D report's ground echo, had already grown
its own rule (FND-297 b, FND-303). Moving every copy into one module fixes today's
tree; this gate is what stops the next copy. A sentence asking people to import
the constant cannot see a new literal. A grep can.

Scanned: every `.rs` file under `crates/`, `apps/` and `bindings/`, outside
`target/`. Exempt: the owner (`crates/nec_model/src/constants.rs`) and
integration-test directories (`tests/`) — a test that restates c0 to check the
solver against an independent formula is an oracle, not a copy. In-file
`#[cfg(test)]` modules are NOT exempt: they compile into the crate that owns the
constant and have no reason to restate it. Line comments are stripped first, so
a doc comment quoting nec2c's `59.96` is not a copy.

Usage:
    check-physical-constants.py [--root DIR]
Exit 0 clean, 1 on any literal (each printed as path:line: pattern), 2 if the
owner file is missing (a gate that scans nothing must not pass).
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

OWNER = Path("crates/nec_model/src/constants.rs")
SCAN_DIRS = ("crates", "apps", "bindings")

_PI = r"(?:std::f64::consts::|f64::consts::|consts::)?PI"
_F = r"(?:\.0*)?"  # optional `.0`, `.00`, ...
# Matched against code with comments stripped and `_` digit separators removed.
PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    ("speed of light 299792458", re.compile(r"299792458")),
    ("speed of light 2.99792…", re.compile(r"\b2\.99792")),
    ("speed of light 3e8", re.compile(r"\b3" + _F + r"[eE]\+?0*8\b")),
    ("eps0 8.854…", re.compile(r"\b8\.854")),
    ("mu0 4e-7·π", re.compile(r"\b4" + _F + r"[eE]-0*7\s*\*\s*" + _PI)),
    ("mu0 π·4e-7", re.compile(_PI + r"\s*\*\s*4" + _F + r"[eE]-0*7\b")),
    ("mu0 4·π·1e-7", re.compile(r"\b4" + _F + r"\s*\*\s*" + _PI + r"\s*\*\s*1" + _F + r"[eE]-0*7\b")),
    ("mu0 1.2566…", re.compile(r"\b1\.2566")),
    ("eta0 376.73…", re.compile(r"\b376\.73")),
    ("eta0 120·π", re.compile(r"\b120" + _F + r"\s*\*\s*" + _PI)),
    ("eta0/2π 59.96", re.compile(r"\b59\.9[56]")),
]


def strip_comment(line: str) -> str:
    """The line up to a `//` that is outside a string literal."""
    in_str = False
    i = 0
    while i < len(line):
        c = line[i]
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_str = False
        elif c == '"':
            in_str = True
        elif line.startswith("//", i):
            return line[:i]
        i += 1
    return line


def normalise(code: str) -> str:
    """Drop `_` digit separators: `299_792_458` → `299792458`."""
    return re.sub(r"(?<=\d)_(?=\d)", "", code)


def scan_file(path: Path, rel: str) -> list[str]:
    hits = []
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        code = normalise(strip_comment(line))
        for name, pat in PATTERNS:
            if pat.search(code):
                hits.append(f"{rel}:{n}: {name}: {line.strip()}")
    return hits


def exempt(rel: Path) -> bool:
    parts = rel.parts
    # `<crates|apps|bindings>/<crate>/tests/…` — a crate's integration tests only.
    integration_test = len(parts) > 3 and parts[2] == "tests"
    return rel == OWNER or "target" in parts or integration_test


def scan(root: Path) -> list[str]:
    hits: list[str] = []
    for top in SCAN_DIRS:
        base = root / top
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*.rs")):
            rel = path.relative_to(root)
            if exempt(rel):
                continue
            hits.extend(scan_file(path, rel.as_posix()))
    return hits


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = ap.parse_args()
    if not (args.root / OWNER).is_file():
        print(f"check-physical-constants: owner {OWNER} not found under {args.root}", file=sys.stderr)
        return 2
    hits = scan(args.root)
    if hits:
        print(
            "physical-constant literals outside nec_model::constants "
            "(use nec_solver::constants / nec_model::constants instead):",
            file=sys.stderr,
        )
        for h in hits:
            print(f"  {h}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

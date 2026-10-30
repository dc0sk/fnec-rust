#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Refuse a hand-built scratch path in the CLI's integration tests.

Tests in one binary run in parallel, so a scratch file's name must be unique
per call, and a name keyed on the clock is not: two `SystemTime::now()` reads
returned the same nanosecond, one test's load file replaced another's, and a
test that had to see a refusal solved a valid load (FND-319; FND-001 was the
same collision in `exec_modes`). `common::TempDeck` now owns uniqueness (pid +
counter, `create_new`). A file that calls `std::env::temp_dir()` itself carries
its own copy of the rule, and a sentence asking for `TempDeck` cannot see a new
one. A grep can.

Scanned: every `.rs` file under `apps/nec-cli/tests/`. Exempt: `common/mod.rs`,
the owner, and the files listed in `scripts/scratch-paths-allowlist.txt` (the 42
that predate this gate, FND-320). The list may only shrink: an entry whose file
no longer calls `temp_dir()`, or no longer exists, is itself an error, so a
migrated file must leave the list in the same change. Line comments are
stripped first, so prose naming `temp_dir()` is not a call.

Usage:
    check-scratch-paths.py [--root DIR]
Exit 0 clean, 1 on any violation (each printed), 2 if the scanned directory or
the owner is missing (a gate that scans nothing must not pass).
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

TESTS = Path("apps/nec-cli/tests")
OWNER = TESTS / "common" / "mod.rs"
ALLOWLIST = Path("scripts/scratch-paths-allowlist.txt")
CALL = re.compile(r"\btemp_dir\s*\(")


def strip_comment(line: str) -> str:
    """The line up to a `//` that is outside a string literal."""
    in_str = False
    i = 0
    while i < len(line):
        c = line[i]
        if c == "\\" and in_str:
            i += 2
            continue
        if c == '"':
            in_str = not in_str
        elif not in_str and line.startswith("//", i):
            return line[:i]
        i += 1
    return line


def calls(path: Path) -> list[int]:
    """1-based lines of `path` that call `temp_dir()` outside a comment."""
    out = []
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if CALL.search(strip_comment(line)):
            out.append(n)
    return out


def read_allowlist(path: Path) -> tuple[dict[str, int], list[str]]:
    """Entries (path -> line number) and format problems."""
    entries: dict[str, int] = {}
    problems: list[str] = []
    for n, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        entry, sep, reason = line.partition("#")
        entry = entry.strip()
        if not sep or not reason.strip():
            problems.append(f"{path}:{n}: {entry!r} gives no reason (`<path>  # <reason>`)")
        if entry in entries:
            problems.append(f"{path}:{n}: {entry} listed twice (first line {entries[entry]})")
        entries[entry] = n
    return entries, problems


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=".", type=Path)
    root = ap.parse_args().root
    tests, owner, allow = root / TESTS, root / OWNER, root / ALLOWLIST
    for p in (tests, owner, allow):
        if not p.exists():
            print(f"check-scratch-paths: {p} is missing; nothing to check", file=sys.stderr)
            return 2

    allowed, problems = read_allowlist(allow)
    seen: set[str] = set()
    for f in sorted(tests.rglob("*.rs")):
        rel = f.relative_to(root).as_posix()
        if f == owner:
            continue
        lines = calls(f)
        if not lines:
            continue
        seen.add(rel)
        if rel not in allowed:
            for n in lines:
                problems.append(
                    f"{rel}:{n}: builds a scratch path with temp_dir(); use common::TempDeck, "
                    f"which owns the name's uniqueness (FND-319)"
                )
    for entry, n in sorted(allowed.items(), key=lambda e: e[1]):
        if entry not in seen:
            problems.append(
                f"{ALLOWLIST}:{n}: {entry} no longer calls temp_dir() (or is gone); "
                f"delete its line — the list only shrinks"
            )

    if problems:
        for p in problems:
            print(p, file=sys.stderr)
        return 1
    print(f"scratch paths OK — {len(seen)} allowlisted file(s), no new hand-built path")
    return 0


if __name__ == "__main__":
    sys.exit(main())

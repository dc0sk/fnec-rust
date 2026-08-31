#!/usr/bin/env python3
"""Check `docs/project/test-catalog.md`'s counts against the real test binaries.

The catalog's per-crate unit-test table and its totals were hand-maintained and
drifted by more than a factor of two: it claimed ~532 `#[test]` functions and
"539 passing across 53 test binaries" while the workspace had grown past 1000
(FND-065, FND-143). A count typed by hand is a claim; this makes it a check.

The numbers come from `cargo test --workspace -- --list`, which enumerates what
the harness will actually run — not from grepping for `#[test]`, which misses
macro-generated cases and counts commented-out ones.

Note the configuration matters and is recorded in the doc: run under
`--workspace`, feature unification turns on `nec_accel/wgpu`, which adds four
lib tests that a bare `cargo test -p nec_accel` does not build at all.

Usage:
    python3 scripts/check-test-catalog-counts.py           # check, exit 1 on drift
    python3 scripts/check-test-catalog-counts.py --list-only  # print measured counts
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "docs/project/test-catalog.md"

# Table row label -> cargo package name whose *unit* (src/) tests it counts.
UNIT_ROWS = {
    "nec_solver": "nec_solver",
    "nec_worker": "nec_worker",
    "nec_report": "nec_report",
    "nec_accel": "nec_accel",
    "nec_parser": "nec_parser",
    "nec_project": "nec_project",
    "nec_model": "nec_model",
    "nec-gui": "nec_gui",
    "apps/nec-cli": "fnec",
}


def measured() -> tuple[Counter, Counter]:
    """(unit counts by crate, integration counts by crate), from the harness."""
    # stderr must be MERGED, not captured separately: cargo prints the
    # "Running <target>" markers on stderr and the test names on stdout, so two
    # separate streams lose the interleaving that attributes a name to a target.
    # Concatenating them afterwards puts every marker after every name and the
    # parse silently attributes nothing — which is what the guard below caught.
    # CARGO_TERM_COLOR=never, because CI sets it to `always` and the ANSI reset
    # lands between "Running" and the target path — `Running\x1b[0m unittests
    # src/lib.rs` matches no regex that expects a space there. The `--list-only`
    # run is clean locally and failed in CI for exactly this reason; the floor
    # below is what turned a silent zero into a loud one. The strip is kept as
    # well, so the parse does not depend on the env var being honoured.
    proc = subprocess.run(
        ["cargo", "test", "--workspace", "--", "--list"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        env={**os.environ, "CARGO_TERM_COLOR": "never"},
    )
    if proc.returncode != 0:
        sys.exit(f"cargo test --list failed (exit {proc.returncode}):\n{proc.stdout[-2000:]}")

    unit: Counter = Counter()
    integration: Counter = Counter()
    current: tuple[bool, str] | None = None
    ansi = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
    for line in ansi.sub("", proc.stdout).splitlines():
        m = re.search(r"Running (?:unittests )?(\S+) \(([^)]+)\)", line)
        if m:
            src, binary = m.group(1), m.group(2).split("/")[-1]
            crate = re.sub(r"-[0-9a-f]{16}$", "", binary)
            current = (src.startswith("src/"), crate)
            continue
        if line.rstrip().endswith(": test") and current is not None:
            is_unit, crate = current
            (unit if is_unit else integration)[crate] += 1
    if not unit:
        sys.exit("no unit tests were enumerated — the parse or the build is wrong")
    return unit, integration


def main() -> int:
    unit, integration = measured()
    total = sum(unit.values()) + sum(integration.values())

    if "--list-only" in sys.argv:
        # Unit keys are crate names (the lib target's binary is named for its
        # crate); integration keys are TEST FILE names, because each `tests/*.rs`
        # is its own binary. They are printed separately so the two are not read
        # as one per-crate table.
        print("unit tests (src/), by crate:")
        for crate in sorted(unit):
            print(f"  {crate:24s} {unit[crate]:4d}")
        print("integration tests (tests/), by test file:")
        for f in sorted(integration):
            print(f"  {f:24s} {integration[f]:4d}")
        print(f"\nunit {sum(unit.values())} + integration "
              f"{sum(integration.values())} = {total}")
        return 0

    text = CATALOG.read_text(encoding="utf-8")
    problems: list[str] = []

    for label, crate in UNIT_ROWS.items():
        want = unit[crate]
        row = next(
            (l for l in text.splitlines()
             if l.startswith("|") and l.split("|")[1].strip().strip("`") == label),
            None,
        )
        if row is None:
            problems.append(f"no unit-table row for `{label}` (measured {want} unit tests)")
            continue
        cell = row.split("|")[2].strip()
        if cell != str(want):
            problems.append(f"`{label}`: table says {cell}, harness reports {want}")

    for name, want in (
        ("unit subtotal", sum(unit.values())),
        ("integration subtotal", sum(integration.values())),
        ("workspace total", total),
    ):
        marker = f"<!-- COUNT:{name.upper().replace(' ', '-')}="
        if marker not in text:
            problems.append(f"missing `{marker}{want} -->` marker in the catalog")
            continue
        stated = text.split(marker, 1)[1].split("-->", 1)[0].strip()
        if stated != str(want):
            problems.append(f"{name}: doc says {stated}, harness reports {want}")

    # LIMIT, stated rather than implied: this checks the per-crate UNIT rows and
    # the three totals. The per-file integration table is NOT checked, because a
    # test file's binary is named for its stem alone and two packages here both
    # ship `tests/current_source_junction.rs`, so a stem key silently merges
    # them. Doing it correctly needs `--message-format=json` to map each
    # executable to its `src_path`. Measured drift in that table as of
    # 2026-08-31: 12 listed rows wrong, ~30 test binaries with no row. FND-143.
    if problems:
        print("docs/project/test-catalog.md counts are stale:")
        for p in problems:
            print(f"  - {p}")
        print("\nre-measure with: python3 scripts/check-test-catalog-counts.py --list-only")
        return 1

    print(f"test catalog counts OK — {total} tests "
          f"({sum(unit.values())} unit + {sum(integration.values())} integration)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

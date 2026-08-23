#!/usr/bin/env python3
"""Keep `docs/project/path-inventory.md` from rotting into fiction.

Two ways an inventory decays into something that reads as coverage while proving
nothing: it cites a test that has since been renamed or deleted, or it marks a path
as a gap without linking anywhere, so the gap is recorded and then forgotten.

This checks references only. Whether a cited test genuinely exercises its path is a
question for the test, not for a grep — a checker claiming otherwise would pass
forever without having looked.

Usage:
  scripts/check-path-inventory.py        # exit 1 on a dangling reference
"""

import re
import subprocess
import sys
from pathlib import Path

INVENTORY = Path("docs/project/path-inventory.md")
LEDGER = Path("docs/project/findings-ledger.md")

# `path/to/file.rs` or `path/to/file.py` mentioned in a table row.
FILE_RE = re.compile(r"`((?:apps|crates|bindings|scripts)/[\w./-]+\.(?:rs|py))`")
# A bare test-function name in backticks, e.g. `every_gui_solve_path_...`.
TESTNAME_RE = re.compile(r"`([a-z][a-z0-9_]{15,})`")
FND_RE = re.compile(r"FND-\d+")


def main() -> int:
    if not INVENTORY.exists():
        print(f"{INVENTORY} is missing", file=sys.stderr)
        return 1
    text = INVENTORY.read_text(encoding="utf-8")
    ledger_ids = set(FND_RE.findall(LEDGER.read_text(encoding="utf-8"))) if LEDGER.exists() else set()

    problems: list[str] = []
    rows = 0
    checked_files = 0
    checked_tests = 0

    for lineno, line in enumerate(text.splitlines(), 1):
        if not re.match(r"^\|\s*\d+\s*\|", line):
            continue
        rows += 1

        for rel in FILE_RE.findall(line):
            checked_files += 1
            if not Path(rel).exists():
                problems.append(f"{INVENTORY}:{lineno}: cites {rel}, which does not exist")

        # Every gap row must point at a finding that is really in the ledger.
        if re.search(r"\*\*NO\*\*", line):
            ids = FND_RE.findall(line)
            if not ids:
                problems.append(
                    f"{INVENTORY}:{lineno}: marked as a gap but cites no FND- finding"
                )
            for fid in ids:
                if fid not in ledger_ids:
                    problems.append(
                        f"{INVENTORY}:{lineno}: cites {fid}, which is not in {LEDGER}"
                    )

        # A named test must exist somewhere in the tree.
        for name in TESTNAME_RE.findall(line):
            if "/" in name or name.endswith(".rs") or name.endswith(".py"):
                continue
            checked_tests += 1
            found = subprocess.run(
                ["git", "grep", "-q", "--", name], capture_output=True
            ).returncode == 0
            if not found:
                problems.append(
                    f"{INVENTORY}:{lineno}: cites test `{name}`, which is nowhere in the tree"
                )

    if rows == 0:
        problems.append(f"{INVENTORY}: no path rows found; the inventory cannot be empty")

    if problems:
        for p in problems:
            print(p, file=sys.stderr)
        return 1
    print(
        f"path inventory OK — {rows} path(s), "
        f"{checked_files} file reference(s) and {checked_tests} test name(s) resolve"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

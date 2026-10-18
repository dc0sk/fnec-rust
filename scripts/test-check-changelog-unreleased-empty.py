#!/usr/bin/env python3
"""Known-pass / known-fail cases for `check-changelog-unreleased-empty.py`.

The checker runs only inside the Mint release tag workflow, so the first time
it could be seen failing would otherwise be at a mint. These cases run in CI
and in check-all instead.

Run: python3 scripts/test-check-changelog-unreleased-empty.py
"""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).parent
spec = importlib.util.spec_from_file_location(
    "checker", HERE / "check-changelog-unreleased-empty.py"
)
checker = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(checker)

HEAD = "# Changelog\n\nPreamble text.\n\n"
RELEASE = "## [0.23.0] - 2026-10-17\n\n### Fixed\n\n- An old entry.\n"

failures: list[str] = []
ran = 0


def check(name: str, got, want) -> None:
    global ran
    ran += 1
    if got != want:
        failures.append(f"{name}: got {got!r}, want {want!r}")


# --- the section reader -----------------------------------------------------
check("empty, release below", checker.unreleased_lines(HEAD + "## [Unreleased]\n\n" + RELEASE), [])
check("empty, blank lines only", checker.unreleased_lines(HEAD + "## [Unreleased]\n\n\n  \n\n" + RELEASE), [])
check("empty, last section", checker.unreleased_lines(HEAD + RELEASE + "\n## [Unreleased]\n"), [])
check("no Unreleased heading", checker.unreleased_lines(HEAD + RELEASE), [])
check(
    "an entry under a heading",
    checker.unreleased_lines(HEAD + "## [Unreleased]\n\n### Fixed\n\n- New.\n\n" + RELEASE),
    ["### Fixed", "- New."],
)
check(
    "a bare line, no heading",
    checker.unreleased_lines(HEAD + "## [Unreleased]\n\nstray text\n" + RELEASE),
    ["stray text"],
)
# The release section's own entries are not the Unreleased section's.
check(
    "release entries not counted",
    checker.unreleased_lines(HEAD + "## [Unreleased]\n" + RELEASE + "\n## [0.22.0]\n\n- Older.\n"),
    [],
)

# --- the command, end to end: exit status is what the workflow reads -----------
with tempfile.TemporaryDirectory() as d:
    for name, body, want in [
        ("cli pass", HEAD + "## [Unreleased]\n\n" + RELEASE, 0),
        ("cli fail", HEAD + "## [Unreleased]\n\n### Added\n\n- New.\n\n" + RELEASE, 1),
    ]:
        p = Path(d) / "changelog.md"
        p.write_text(body, encoding="utf-8")
        r = subprocess.run(
            [sys.executable, str(HERE / "check-changelog-unreleased-empty.py"), str(p)],
            capture_output=True,
            text=True,
        )
        check(name, r.returncode, want)

if failures:
    for f in failures:
        print(f"FAIL {f}", file=sys.stderr)
    print(f"check-changelog-unreleased-empty self-test: {len(failures)} of {ran} failed", file=sys.stderr)
    sys.exit(1)
print(f"check-changelog-unreleased-empty self-test: {ran} cases OK")

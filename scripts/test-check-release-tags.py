#!/usr/bin/env python3
"""Known-pass / known-fail cases for `check-release-tags.py`.

The first committed self-test among this repo's `check-*` scripts, and it exists
for the same reason the checker does: every gate that let FND-043 and FND-044
through was passing honestly while looking in the wrong place. A checker nobody
has watched fail is not yet evidence.

Run: python3 scripts/test-check-release-tags.py
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "checker", Path(__file__).parent / "check-release-tags.py"
)
checker = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(checker)

CARGO_WORKSPACE = '[workspace.package]\nversion = "0.15.0"\n'
CARGO_PACKAGE = '[package]\nname = "x"\nversion = "0.3.0"\n'
PYPROJECT = '[project]\nname = "fnec_py"\nversion = "0.6.0"\n'

failures: list[str] = []


def check(name: str, got, want) -> None:
    if got != want:
        failures.append(f"{name}: got {got!r}, want {want!r}")


# --- the manifest reader ----------------------------------------------------
# It has to cope with both manifest eras, and must not silently accept a
# pyproject `[project]` table as a Cargo version.
check("workspace layout", checker.declared_version(CARGO_WORKSPACE), "0.15.0")
check("package layout", checker.declared_version(CARGO_PACKAGE), "0.3.0")
check("pyproject is not a Cargo manifest", checker.declared_version(PYPROJECT), None)
check("malformed toml", checker.declared_version("not = = toml"), None)

# --- the wheel reader -------------------------------------------------------
check("pyproject", checker.wheel_version(PYPROJECT), "0.6.0")
check("a Cargo manifest is not a pyproject", checker.wheel_version(CARGO_WORKSPACE), None)

# --- the tag pattern --------------------------------------------------------
# Anchored: `v0.2.0-phase2-pre-hallen-reform` would otherwise be read as
# "v0.2.0" and pass by coincidence, hiding the parsing bug.
check("release tag", bool(checker.RELEASE_TAG.match("v0.15.0")), True)
check("phase tag rejected", bool(checker.RELEASE_TAG.match("v0.2.0-phase2-pre-hallen-reform")), False)
check("baseline tag rejected", bool(checker.RELEASE_TAG.match("phase1-baseline")), False)

# --- semver ordering --------------------------------------------------------
# String order puts "0.9.0" after "0.15.0"; the newest-exemption depends on this.
check(
    "newest is semver-max, not lexical",
    max(["0.9.0", "0.15.0", "0.4.0"], key=checker.semver_key),
    "0.15.0",
)

# --- the grandfather sets ---------------------------------------------------
# Frozen historical facts. If either grows, the check failed and someone
# silenced it rather than fixing a release.
check("untagged releases", checker.UNTAGGED_RELEASES,
      frozenset({"0.4.0", "0.5.0", "0.6.0", "0.8.0", "0.9.0"}))
check("binding mismatch", checker.BINDING_MISMATCH_TAGS, frozenset({"v0.14.0"}))

if failures:
    for f in failures:
        print(f"FAIL {f}", file=sys.stderr)
    sys.exit(1)
print(f"check-release-tags self-test OK — {13 - len(failures)} case(s)")

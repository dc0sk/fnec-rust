#!/usr/bin/env python3
"""Release tags exist, and each one names the version its tree declares.

Two failures this repo has actually shipped:

* **v0.4.0, v0.5.0, v0.6.0, v0.8.0 and v0.9.0 were released without tags**
  (FND-043). They have changelog sections and no ref, so there is nothing to
  check out, compare against, or link — the Keep-a-Changelog compare links exist
  only for the contiguous tagged range because of it.
* **v0.15.0 was first tagged at a commit whose tree said `fnec_py` 0.4.0**
  (FND-044). Caught by hand, minutes before publishing, by building the wheel and
  reading its filename. Nothing would have caught it otherwise.

This is a *detector*, not a preventer. Only automation prevents a forgotten tag —
a workflow that mints one when a version bump merges. That is a policy decision
for the maintainer (this project's convention is that releases are cut
explicitly), so it is proposed separately. The detector keeps its value either
way: it guards against a tag being *deleted*, and against that workflow being
silently disabled.

Run from the repository root, with tags fetched.
"""

from __future__ import annotations

import re
import subprocess
import sys
import tomllib
from pathlib import Path

CHANGELOG = Path("docs/changelog.md")

# Anchored on purpose. `v0.2.0-phase2-pre-hallen-reform` matches an unanchored
# `v\d+\.\d+\.\d+` and would be checked as "v0.2.0" — and would then *pass* by
# coincidence, since its tree does say 0.2.0, hiding the parsing bug.
RELEASE_TAG = re.compile(r"^v(\d+\.\d+\.\d+)$")
CHANGELOG_VERSION = re.compile(r"^## \[(\d+\.\d+\.\d+)\]", re.MULTILINE)

# Released before tagging was consistent (FND-043). Retroactive tagging means
# guessing which commit each was cut from, which is worse than the gap.
#
# A frozen set, not a `BEFORE` cutoff, for a reason specific to this data: the
# missing tags *interleave* with present ones — 0.3.0 tagged, 0.4-0.6 not, 0.7.0
# tagged, 0.8-0.9 not. A cutoff would also stop guarding v0.3.0 and v0.7.0, and
# tag deletion is a live failure mode here: v0.15.0 was deleted and re-pushed
# during its own release. This set can never grow; a sixth entry would mean the
# check had already failed and someone silenced it.
UNTAGGED_RELEASES = frozenset({"0.4.0", "0.5.0", "0.6.0", "0.8.0", "0.9.0"})

# v0.14.0 shipped `pyproject.toml` 0.4.0 against a bindings crate saying 0.5.0
# (FND-044) — so its wheel was labelled 0.4.0 while carrying 0.5.0's breaking
# behaviour. The tag is published and immutable; re-pointing it to satisfy a
# checker would be worse than the defect it records. Separate from the set above
# because it exempts a different sub-check for a different reason.
BINDING_MISMATCH_TAGS = frozenset({"v0.14.0"})

# Distinct from a finding: the environment simply has no tags to check.
EXIT_NO_TAGS = 2


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, check=False
    ).stdout


def file_at_tag(tag: str, path: str) -> str | None:
    """The file's contents at `tag`, or None if it does not exist there."""
    probe = subprocess.run(
        ["git", "cat-file", "-e", f"{tag}:{path}"], capture_output=True, check=False
    )
    if probe.returncode != 0:
        return None
    return git("show", f"{tag}:{path}")


def declared_version(toml_text: str) -> str | None:
    """The package version, wherever this era of the manifest keeps it.

    Not a grep. At HEAD the workspace version lives under `[workspace.package]`;
    older tags differ. A first-match `^version =` grep happens to work on every
    tag today, which is exactly how a check earns false confidence — the sibling
    `check-binding-version.py` exists because a grep was looking in the wrong
    file entirely.
    """
    try:
        data = tomllib.loads(toml_text)
    except tomllib.TOMLDecodeError:
        return None
    for table in (("workspace", "package"), ("package",)):
        node = data
        for key in table:
            node = node.get(key, {}) if isinstance(node, dict) else {}
        if isinstance(node, dict) and isinstance(node.get("version"), str):
            return node["version"]
    return None


def wheel_version(pyproject_text: str) -> str | None:
    """The version maturin stamps onto the built wheel.

    A separate accessor from `declared_version` on purpose: `pyproject.toml`
    keeps it under `[project]`, and reusing the Cargo reader with `project` added
    to its search list would silently accept the wrong table in either file.
    """
    try:
        data = tomllib.loads(pyproject_text)
    except tomllib.TOMLDecodeError:
        return None
    version = data.get("project", {}).get("version")
    return version if isinstance(version, str) else None


def semver_key(v: str) -> tuple[int, ...]:
    return tuple(int(p) for p in v.split("."))


def main() -> int:
    if not CHANGELOG.exists():
        print(f"{CHANGELOG} is missing", file=sys.stderr)
        return 1

    changelog_versions = CHANGELOG_VERSION.findall(CHANGELOG.read_text(encoding="utf-8"))
    if not changelog_versions:
        print(f"{CHANGELOG}: no released versions found", file=sys.stderr)
        return 1

    tags = {t for t in git("tag", "-l").split() if RELEASE_TAG.match(t)}

    # Fail closed, but say which kind of failure it is. A shallow or --no-tags
    # checkout cannot run this check at all, and reporting that as a release
    # hygiene finding would be a lie; skipping silently would be the fail-open
    # this repo keeps getting bitten by.
    if not tags:
        print(
            "no release tags are visible — this is an unfetched-tags environment, "
            "not a release-hygiene finding.\nRun `git fetch --tags` (CI needs "
            "actions/checkout with fetch-depth: 0).",
            file=sys.stderr,
        )
        return EXIT_NO_TAGS

    problems: list[str] = []

    # The in-flight release: its section is written before its tag exists, so a
    # release PR would otherwise fail its own gate. Exempt only when it really is
    # the version being released — "whatever section is newest" would also excuse
    # a typo'd 0.61.0 nobody meant to add.
    head_version = declared_version(Path("Cargo.toml").read_text(encoding="utf-8"))
    newest = max(changelog_versions, key=semver_key)
    in_flight = newest if newest == head_version else None

    for version in sorted(set(changelog_versions), key=semver_key):
        if version in UNTAGGED_RELEASES or version == in_flight:
            continue
        if f"v{version}" not in tags:
            problems.append(
                f"{CHANGELOG} has a [{version}] section but no v{version} tag — "
                f"a released version with no ref cannot be checked out, compared "
                f"against, or linked"
            )

    for tag in sorted(tags, key=lambda t: semver_key(t[1:])):
        manifest = file_at_tag(tag, "Cargo.toml")
        if manifest is None:
            problems.append(f"{tag}: no Cargo.toml at this tag")
            continue
        declared = declared_version(manifest)
        if declared != tag[1:]:
            problems.append(
                f"{tag} names {tag[1:]} but its tree declares {declared} — "
                f"the tag points at the wrong commit"
            )

        if tag in BINDING_MISMATCH_TAGS:
            continue
        # Compared against the *bindings* crate, never the tag name: fnec_py
        # versions independently (0.6.0 at v0.15.0). "Everything must say the tag
        # version" is the naive reading a future editor will reach for.
        pyproject = file_at_tag(tag, "bindings/fnec_py/pyproject.toml")
        crate = file_at_tag(tag, "bindings/fnec_py/Cargo.toml")
        if pyproject is None or crate is None:
            continue  # predates the bindings
        wheel = wheel_version(pyproject)
        crate_version = declared_version(crate)
        if wheel is None or crate_version is None:
            problems.append(f"{tag}: could not read a bindings version to compare")
        elif wheel != crate_version:
            problems.append(
                f"{tag}: the wheel would be labelled {wheel} while the "
                f"crate says {crate_version}"
            )

    if problems:
        for p in problems:
            print(p, file=sys.stderr)
        return 1

    checked = len(set(changelog_versions)) - len(UNTAGGED_RELEASES)
    print(
        f"release tags OK — {len(tags)} tag(s), {checked} released version(s) "
        f"checked, {len(UNTAGGED_RELEASES)} grandfathered"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

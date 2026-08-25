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
import os
import subprocess
import sys
import tempfile
import time
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
ran = 0


def check(name: str, got, want) -> None:
    """Count here, not at the print.

    The banner used to interpolate a hardcoded 13 — and there were 12 cases. The
    figure survived into a PR body and a CI log as though the log had confirmed
    it, in a change whose whole argument is that the previous gates were passing
    honestly while looking in the wrong place. A self-test that cannot miscount
    is the only kind worth quoting.
    """
    global ran
    ran += 1
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

# --- end-to-end, against real git repositories --------------------------------
# The cases above exercise pure helpers. Everything that decides an outcome —
# the in-flight exemption, the grandfather sets, both loops — lives in `main()`,
# and until these fixtures existed it could be hollowed out with every case
# above still green: `for tag in []` still printed the right banner, because the
# banner counts the tag *set*, not what was looped.


def build_repo(root: Path, versions: list[str], tags: dict[str, str], head: str) -> None:
    """A throwaway repo: a changelog listing `versions`, tags at `tags`, Cargo at `head`.

    `tags` maps a tag name to the version its tree should declare, so a tag can
    deliberately be made to point at the wrong thing.
    """
    subprocess.run(["git", "init", "-q", str(root)], check=True)
    env = {
        **os.environ,
        "GIT_AUTHOR_NAME": "t",
        "GIT_AUTHOR_EMAIL": "t@e",
        "GIT_COMMITTER_NAME": "t",
        "GIT_COMMITTER_EMAIL": "t@e",
    }

    def git(*a: str) -> None:
        subprocess.run(["git", "-C", str(root), *a], check=True, env=env,
                       capture_output=True)

    (root / "docs").mkdir()
    for tag, version in tags.items():
        (root / "Cargo.toml").write_text(f'[workspace.package]\nversion = "{version}"\n')
        (root / "docs" / "changelog.md").write_text("# Changelog\n")
        git("add", "-A")
        git("commit", "-qm", f"at {tag}")
        git("tag", "-a", tag, "-m", tag)

    sections = "\n".join(f"## [{v}] — 2026-01-01 — x\n" for v in versions)
    (root / "docs" / "changelog.md").write_text(f"# Changelog\n\n{sections}")
    (root / "Cargo.toml").write_text(f'[workspace.package]\nversion = "{head}"\n')
    git("add", "-A")
    git("commit", "-qm", "head")


def run_checker(root: Path) -> int:
    return subprocess.run(
        [sys.executable, str(Path(__file__).parent.resolve() / "check-release-tags.py")],
        cwd=root,
        capture_output=True,
    ).returncode


def e2e(name: str, versions: list[str], tags: dict[str, str], head: str, want: int) -> None:
    global ran
    ran += 1
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "repo"
        build_repo(root, versions, tags, head)
        got = run_checker(root)
    if got != want:
        failures.append(f"{name}: exit {got}, want {want}")


def e2e_aged(name: str, versions: list[str], tags: dict[str, str], head: str,
             changelog_age_days: int, want: int) -> None:
    """Like `e2e`, but backdates the commit that introduces the newest section.

    The checker dates a version by when the changelog first carried its heading,
    so the fixture has to commit that heading with an old timestamp.
    """
    global ran
    ran += 1
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "repo"
        build_repo(root, versions, tags, head)
        when = int(time.time()) - changelog_age_days * 86400
        stamp = time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime(when))
        env = {
            **os.environ,
            "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e",
            "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@e",
            "GIT_AUTHOR_DATE": stamp, "GIT_COMMITTER_DATE": stamp,
        }
        # Rewrite the final commit with the backdated timestamp.
        subprocess.run(["git", "-C", str(root), "commit", "-q", "--amend",
                        "--no-edit", "--date", stamp],
                       check=True, env=env, capture_output=True)
        got = run_checker(root)
    if got != want:
        failures.append(f"{name}: exit {got}, want {want}")


# A healthy repo: every released version tagged, each tag naming its own tree.
e2e("all tagged", ["1.0.0", "1.1.0"], {"v1.0.0": "1.0.0", "v1.1.0": "1.1.0"}, "1.1.0", 0)

# The finding itself: a released version with no tag.
e2e("untagged release", ["1.0.0", "1.1.0"], {"v1.1.0": "1.1.0"}, "1.1.0", 1)

# The release in flight — its section exists before its tag does, so a release PR
# must not fail its own gate.
e2e("in flight", ["1.0.0", "1.1.0"], {"v1.0.0": "1.0.0"}, "1.1.0", 0)

# ...but only while it really is the version being released. A stray section is
# not excused just for being newest.
e2e("stray section", ["1.0.0", "1.1.0"], {"v1.0.0": "1.0.0"}, "1.0.0", 1)

# A tag naming a version its tree does not declare — the v0.15.0 incident.
e2e("mispointed tag", ["1.0.0"], {"v1.0.0": "0.9.0"}, "1.0.0", 1)

# A tag on the binding-mismatch grandfather list must still have its *pointing*
# checked. The exemption is narrow — it excuses v0.14.0's wheel label, not the
# tag itself — and moving its `continue` one check earlier silently widens it to
# everything, which passes on the real repo because v0.14.0's Cargo is correct.
e2e(
    "a grandfathered tag is still checked for pointing",
    ["0.14.0"],
    {"v0.14.0": "0.9.0"},
    "0.14.0",
    1,
)

# The in-flight exemption must expire. "Newest section, no tag" is a release in
# flight for a while and a deleted-or-never-minted tag after that; only age tells
# them apart, and without this the exemption is a permanent blind spot at exactly
# the tag anyone has ever actually deleted.
e2e_aged(
    "an in-flight release ages into a finding",
    versions=["1.0.0", "1.1.0"],
    tags={"v1.0.0": "1.0.0"},
    head="1.1.0",
    changelog_age_days=checker.MAX_UNTAGGED_AGE_DAYS + 3,
    want=1,
)
e2e_aged(
    "a release cut today is still in flight",
    versions=["1.0.0", "1.1.0"],
    tags={"v1.0.0": "1.0.0"},
    head="1.1.0",
    changelog_age_days=0,
    want=0,
)

# The age branch must only apply when the tag is ABSENT. An in-flight version
# that already has its tag is simply fine, and reporting "no tag for N days"
# about a tagged release would be the checker inventing a finding — which it
# would have done for v0.15.0 eight days after release.
e2e_aged(
    "an old but tagged newest release is fine",
    versions=["1.0.0", "1.1.0"],
    tags={"v1.0.0": "1.0.0", "v1.1.0": "1.1.0"},
    head="1.1.0",
    changelog_age_days=checker.MAX_UNTAGGED_AGE_DAYS + 30,
    want=0,
)

# No release tags at all: an environment that cannot run this check, which must
# be distinguishable from a finding.
e2e("no tags is exit 2", ["1.0.0"], {}, "1.0.0", 2)

if failures:
    for f in failures:
        print(f"FAIL {f}", file=sys.stderr)
    sys.exit(1)
print(f"check-release-tags self-test OK — {ran} case(s)")

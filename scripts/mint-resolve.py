#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Resolve the version and the commit the Mint release tag workflow will tag.

Run from the repository root, with `main` checked out (HEAD is main's tip).
Reads two environment variables, both optional:

  REQUESTED  the `version` input: if given, it must equal the version the
             commit to tag declares.
  SHA_INPUT  the `sha` input: a full 40-hex commit that is an ancestor of HEAD.
             Empty means tip mode, which tags HEAD — the behaviour before
             FND-322.

Prints `version=`, `sha=`, `mode=tip|sha`, `behind=N` and `tip=` on stdout,
for `$GITHUB_OUTPUT`. In sha mode it also writes what the tag leaves out —
`git log --oneline SHA..HEAD` — to stderr (the log) and to
`$GITHUB_STEP_SUMMARY` when that is set. A refusal is an `::error::` line on
stderr and exit 1.

Why a sha mode exists (FND-322): the workflow used to tag only the tip, and
refuses while the tip's `[Unreleased]` has content. Once a release has waited
past `check-release-tags.py`'s MAX_UNTAGGED_AGE_DAYS, every PR's `docs
contract` job fails on the age, so the docs PR that would fold later entries
under the release heading cannot merge either. v0.25.0 was tagged by hand at its
release commit because neither route remained. Tagging the release commit
itself through this workflow is that route, with every check still run at it.

Run the self-test: python3 scripts/test-mint-resolve.py
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import cargo_version  # noqa: E402  (the one version parser, FND-067)

# Full 40 lowercase hex and nothing else, checked BEFORE any git call. Two jobs:
# an abbreviation is refused on its form, not on whether git happens to find it
# ambiguous today (a prefix that is unique now can stop being unique, and the tag
# must name exactly the commit the human read); and the value is a
# workflow_dispatch input, so this pattern is also the injection boundary — it
# reaches git only as a list argument, and only after it is known to be hex.
FULL_SHA = re.compile(r"[0-9a-f]{40}")


def git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], capture_output=True, text=True, check=False)


def refuse(message: str) -> int:
    print(f"::error::{message}", file=sys.stderr)
    return 1


def head_sha() -> str:
    return subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True,
                          text=True, check=True).stdout.strip()


def tip_mode(requested: str) -> int:
    # Tag main's tip, not the commit that introduced the version.
    #
    # The alternative — resolve the bump commit — was tried and is wrong here,
    # checkably: for 0.15.0 it picks 2f51d63, the merge of the release PR, whose
    # tree carries the FND-044 wheel-label defect. The real v0.15.0 sits at
    # 1354fda, two commits later, because the defect was found and fixed before
    # tagging. A release that needs a post-bump fix has a bump commit that is
    # *permanently* the wrong tag target, so "always tag the bump" could never
    # mint that release at all.
    #
    # Tagging the tip is also what a human does: pressing this button means
    # "main is ready to be this release". What makes it safe is not the choice
    # of commit but the verification in the workflow, which runs at whatever
    # commit is about to be tagged. The sha mode below does not change this
    # default; it is for the case where later merges must NOT ship.
    head = cargo_version.declared_version(Path("Cargo.toml").read_text(encoding="utf-8"))
    if head is None:
        return refuse("Cargo.toml on main declares no version")
    version = requested or head
    if version != head:
        return refuse(f"requested {version} but main declares {head}. "
                      f"Tag the version main is on, or bump main first.")
    sha = head_sha()
    print(f"version={version}")
    print(f"sha={sha}")
    print("mode=tip")
    print("behind=0")
    print(f"tip={sha}")
    return 0


def sha_mode(requested: str, sha: str) -> int:
    if not FULL_SHA.fullmatch(sha):
        return refuse("sha must be a full 40-character lowercase hex commit id "
                      "(an abbreviation, a ref name or anything else is refused)")
    kind = git("cat-file", "-t", sha).stdout.strip()
    if kind != "commit":
        return refuse(f"sha {sha} names {kind or 'no object'}, not a commit")
    if git("merge-base", "--is-ancestor", sha, "HEAD").returncode != 0:
        return refuse(f"sha {sha} is not an ancestor of main — only a commit "
                      f"that reached main can be tagged")

    # The version is the SHA's tree's, never main's. When main has already been
    # bumped to the next version, check-release-tags.py reports the older release
    # as "a section but no tag", with no in-flight grace, and a tag at an
    # ancestor commit is the only thing that resolves it — so comparing against
    # main's version here would refuse exactly the case this mode exists for.
    version = cargo_version.version_at(sha)
    if version is None:
        return refuse(f"the tree at {sha} declares no version in Cargo.toml")
    if requested and requested != version:
        return refuse(f"requested {requested} but the tree at {sha} declares {version}")

    tip = head_sha()
    behind = int(git("rev-list", "--count", f"{sha}..HEAD").stdout.strip())
    if behind == 0:
        report = "The requested commit is main's tip.\n"
    else:
        # Commit subjects are not shell-safe; they travel only through Python
        # strings and files, never into a shell line.
        log = git("log", "--oneline", "--no-decorate", f"{sha}..HEAD").stdout
        report = (f"{behind} commit(s) on main after the tagged commit are not "
                  f"in v{version}:\n\n{log}")
    sys.stderr.write(report)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(f"### Minting v{version} at {sha}\n\n```\n{report}```\n")

    print(f"version={version}")
    print(f"sha={sha}")
    print("mode=sha")
    print(f"behind={behind}")
    print(f"tip={tip}")
    return 0


def main() -> int:
    requested = os.environ.get("REQUESTED", "").strip()
    sha = os.environ.get("SHA_INPUT", "")
    if sha == "":
        return tip_mode(requested)
    return sha_mode(requested, sha)


if __name__ == "__main__":
    sys.exit(main())

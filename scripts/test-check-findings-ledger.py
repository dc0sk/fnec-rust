#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Known-pass / known-fail cases for `check-findings-ledger.py`'s commit rule.

A cited commit must be an ancestor of HEAD or carry its PR number (FND-317). A
rule that matches no citation passes every ledger, so each phrasing the ledger
cites commits in is planted here with a hash that is NOT on HEAD and must fail;
the same phrasing with an ancestor, or with `(#N)` after a dead hash, must pass;
and a shallow clone or a directory outside git must exit 2, not 0.

A `fixed` row must also carry a PR number: a hash alone, even an ancestor, is
not a citation of the fix (FND-310's row passed on its measurement base). So the
phrasing cases below end in `; #1.`, to test the ancestry rule on its own, and
the rows that owe a PR and carry only a data-shaped hash must fail.

Run: python3 scripts/test-check-findings-ledger.py
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
CHECKER = HERE / "check-findings-ledger.py"
ENV = {
    **os.environ,
    "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t",
    "GIT_COMMITTER_EMAIL": "t@t", "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_SYSTEM": os.devnull,
}

failures: list[str] = []
ran = 0


def git(cwd: Path, *args: str) -> str:
    r = subprocess.run(["git", *args], cwd=cwd, env=ENV, capture_output=True, text=True, check=True)
    return r.stdout.strip()


def check(cwd: Path, evidence: str) -> int:
    (cwd / "ledger.md").write_text(
        "| ID | Found | State | Finding | Evidence / owner |\n"
        "|:---|:------|:------|:--------|:-----------------|\n"
        f"| FND-001 | 2026-01-01 | fixed | a finding | {evidence} |\n"
    )
    r = subprocess.run(
        [sys.executable, str(CHECKER), "--ledger", "ledger.md"],
        cwd=cwd, env=ENV, capture_output=True, text=True,
    )
    return r.returncode


def expect(name: str, cwd: Path, evidence: str, want: int) -> None:
    global ran
    ran += 1
    got = check(cwd, evidence)
    if got != want:
        failures.append(f"{name}: exit {got}, want {want} -- evidence {evidence!r}")


with tempfile.TemporaryDirectory() as d:
    repo = Path(d) / "repo"
    repo.mkdir()
    git(repo, "init", "-q", "-b", "main")
    git(repo, "commit", "-q", "--allow-empty", "-m", "base")
    git(repo, "commit", "-q", "--allow-empty", "-m", "on main")
    on_main = git(repo, "rev-parse", "--short=7", "HEAD")
    # A branch commit that is then left behind, as a squash-merge leaves it.
    git(repo, "checkout", "-q", "-b", "side", "HEAD~1")
    git(repo, "commit", "-q", "--allow-empty", "-m", "branch only")
    off_main = git(repo, "rev-parse", "--short=7", "HEAD")
    git(repo, "checkout", "-q", "main")
    dead = "deadbee"  # in no object store
    digits = "2503790"  # an all-digit hash, as FND-121 cited one

    # Every citation phrasing the ledger uses (known-fail with a non-ancestor).
    phrasings = [
        "Fixed in {h}: the fix.",
        "**FIXED in {h} (this PR).**",
        "**Part (a) fixed in {h}:** the fix.",
        "Fixed in this PR ({h}): the fix.",
        "Sabotages ({h}): `x` reddens.",
        "After the ruling ({h}: one predicate).",
        "fixed, squashed to {h}.",
        "measured on main {h}, then.",
        "the release commit {h} passes.",
        "Reproduced on `{h}`.",
        "fixed in {h}.",
    ]
    phrasings = [p + "; #1." for p in phrasings]
    for p in phrasings:
        expect(f"fail: non-ancestor in {p!r}", repo, p.format(h=off_main), 1)
        expect(f"fail: missing commit in {p!r}", repo, p.format(h=dead), 1)
        expect(f"pass: ancestor in {p!r}", repo, p.format(h=on_main), 0)
    expect("fail: all-digit missing hash", repo, f"fixed in {digits} (this PR); #1.", 1)
    expect("fail: full 40-hex non-ancestor", repo,
           f"fixed in {git(repo, 'rev-parse', off_main)}; #1.", 1)

    # A dead hash that carries its PR number (known-pass).
    expect("pass: dead hash (#N)", repo, f"Fixed in {dead} (#12).", 0)
    expect("pass: dead hash (PR #N, squashed)", repo,
           f"fixed in {dead} (PR #12, squashed to {on_main}): x.", 0)
    expect("pass: dead hash nested (PR #N)", repo, f"silent ({dead} (PR #12, squashed to {on_main})).", 0)
    expect("pass: non-ancestor (#N)", repo, f"Fixed in {off_main} (#12).", 0)
    expect("pass: PR only", repo, "Fixed in #12 (this PR).", 0)
    # #N far from the hash does not excuse it.
    expect("fail: #N not attached", repo, f"Fixed in {dead} (this PR), see #12.", 1)

    # A fixed row whose only hash is data, or an ancestor with no PR (known-fail).
    expect("fail: fixed, the hash is the measurement base", repo,
           f"**FIXED (this PR)** by deleting the copy; agreed on main {on_main}.", 1)
    expect("fail: fixed in an ancestor, no PR", repo, f"Fixed in {on_main}.", 1)
    expect("pass: the same with (#N)", repo,
           f"**FIXED (this PR)** by deleting the copy; agreed on main {on_main} (#12).", 0)
    expect("pass: ancestor; #N", repo, f"Fixed in {on_main}; #12.", 0)

    # Not commit citations (known-pass): prose mentions and numbers.
    expect("pass: prose list", repo, f"says NONE of {dead}, {off_main} or {dead} is reachable; #1.", 0)
    expect("pass: ed25519 / sizes / floats", repo,
           "SSH ed25519; allocation of 2147483648 bytes; was 8.854187817e-12; #1.", 0)

    # Without full history the rule cannot be decided: exit 2, never a green.
    shallow = Path(d) / "shallow"
    subprocess.run(["git", "clone", "-q", "--depth", "1", f"file://{repo}", str(shallow)],
                   env=ENV, check=True, capture_output=True)
    expect("exit 2: shallow clone", shallow, f"Fixed in {on_main}.", 2)
    outside = Path(d) / "nogit"
    outside.mkdir()
    env_no_git = dict(ENV, GIT_CEILING_DIRECTORIES=d)
    ran += 1
    (outside / "ledger.md").write_text("| FND-001 | 2026-01-01 | fixed | a | #1 |\n")
    r = subprocess.run([sys.executable, str(CHECKER), "--ledger", "ledger.md"],
                       cwd=outside, env=env_no_git, capture_output=True, text=True)
    if r.returncode != 2:
        failures.append(f"exit 2: outside git: exit {r.returncode}, want 2")

if failures:
    for f in failures:
        print(f"FAIL {f}", file=sys.stderr)
    print(f"{len(failures)} of {ran} case(s) failed", file=sys.stderr)
    sys.exit(1)
print(f"check-findings-ledger self-test OK — {ran} case(s)")

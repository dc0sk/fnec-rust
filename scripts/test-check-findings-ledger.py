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

Rule A (FND-323) asks a row that turns `fixed` against a base to name its fixing
PR (`fixed in #N`). The scratch repo's root commit therefore commits a ledger
whose FND-001 is already `fixed` with no such phrase, and every `check()` passes
`--base` at it: FND-001 is grandfathered, so the ancestry cases still test the
ancestry rule alone (one mechanism per case). Rule A's own cases use `check2()`,
which puts a new FND-002 above FND-001 or flips FND-001 from a base where it is
`open`; rule B (the placeholder ban) and `--pr` (narrow C) have their own cases,
and the default base -- `merge-base HEAD origin/main` -- is proven once.

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
    r = subprocess.run(["git", *args], cwd=cwd, env=ENV, capture_output=True, text=True,
                       check=True, stdin=subprocess.DEVNULL)
    return r.stdout.strip()


HEADER = (
    "| ID | Found | State | Finding | Evidence / owner |\n"
    "|:---|:------|:------|:--------|:-----------------|\n"
)
# The grandfathered baseline: fixed, citing a PR, with no `fixed in #N` phrase.
BASELINE = HEADER + "| FND-001 | 2026-01-01 | fixed | a finding | #1 |\n"
BASE = ""  # the root commit, which carries BASELINE; set below


def run_checker(cwd: Path, *extra: str) -> int:
    r = subprocess.run(
        [sys.executable, str(CHECKER), "--ledger", "ledger.md", *extra],
        cwd=cwd, env=ENV, capture_output=True, text=True,
    )
    return r.returncode


def check(cwd: Path, evidence: str) -> int:
    (cwd / "ledger.md").write_text(
        HEADER + f"| FND-001 | 2026-01-01 | fixed | a finding | {evidence} |\n"
    )
    return run_checker(cwd, "--base", BASE)


def expect(name: str, cwd: Path, evidence: str, want: int) -> None:
    global ran
    ran += 1
    got = check(cwd, evidence)
    if got != want:
        failures.append(f"{name}: exit {got}, want {want} -- evidence {evidence!r}")


def check2(cwd: Path, rows: str, *extra: str) -> int:
    """A ledger of the given rows (newest first) against `extra`'s base."""
    (cwd / "ledger.md").write_text(HEADER + rows)
    return run_checker(cwd, *extra)


def expect2(name: str, cwd: Path, rows: str, want: int, *extra: str) -> None:
    global ran
    ran += 1
    got = check2(cwd, rows, *extra)
    if got != want:
        failures.append(f"{name}: exit {got}, want {want} -- rows {rows!r} {extra}")


def commit_tree(cwd: Path, ledger: str | None) -> str:
    """A parentless commit holding `ledger` as ledger.md (or nothing), HEAD untouched."""
    if ledger is None:
        tree = git(cwd, "mktree")  # stdin is empty: the empty tree
    else:
        blob = subprocess.run(["git", "hash-object", "-w", "--stdin"], cwd=cwd, env=ENV,
                              input=ledger, capture_output=True, text=True, check=True)
        tree = subprocess.run(["git", "mktree"], cwd=cwd, env=ENV, capture_output=True,
                              text=True, check=True,
                              input=f"100644 blob {blob.stdout.strip()}\tledger.md\n").stdout.strip()
    return git(cwd, "commit-tree", tree, "-m", "a base")


def row(fid: str, state: str, evidence: str) -> str:
    return f"| {fid} | 2026-01-01 | {state} | a finding | {evidence} |\n"


with tempfile.TemporaryDirectory() as d:
    repo = Path(d) / "repo"
    repo.mkdir()
    git(repo, "init", "-q", "-b", "main")
    (repo / "ledger.md").write_text(BASELINE)
    git(repo, "add", "ledger.md")
    git(repo, "commit", "-q", "-m", "base")
    BASE = git(repo, "rev-parse", "HEAD")
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

    # Rule A: a row that turns fixed names its fixing PR (FND-323).
    g = row("FND-001", "fixed", "#1")  # the grandfathered row, as at BASE
    b = ("--base", BASE)
    expect2("A fail: new fixed row cites only an unrelated #N", repo,
            row("FND-002", "fixed", "Builds on #656; done.") + g, 1, *b)
    for ev in ("**FIXED in #657 (this PR).** done", "Fixed by PR #657", "fixed in pr #657"):
        expect2(f"A pass: new fixed row, {ev!r}", repo, row("FND-002", "fixed", ev) + g, 0, *b)
    expect2("A fail: the phrase in the Finding column, not the evidence", repo,
            "| FND-002 | 2026-01-01 | fixed | fixed in #657 | Builds on #656. |\n" + g, 1, *b)
    base_open = commit_tree(repo, HEADER + row("FND-001", "open", "Builds on #656."))
    expect2("A fail: open -> fixed with no phrase (the FND-320 shape)", repo,
            row("FND-001", "fixed", "Builds on #656; done."), 1, "--base", base_open)
    expect2("A pass: open -> fixed with the phrase", repo,
            row("FND-001", "fixed", "Builds on #656. **FIXED in #657 (this PR).**"), 0,
            "--base", base_open)
    expect2("A pass: grandfathered row unchanged", repo, g, 0, *b)
    # Grandfathering is by state at the base, not by text: an edited row that
    # was already fixed is not re-judged.
    expect2("A pass: grandfathered row edited, still no phrase", repo,
            row("FND-001", "fixed", "Builds on #656, reworded."), 0, *b)
    no_ledger = commit_tree(repo, None)
    expect2("A fail: no ledger at the base, fixed row without the phrase", repo,
            g, 1, "--base", no_ledger)
    expect2("A pass: no ledger at the base, fixed row with the phrase", repo,
            row("FND-001", "fixed", "Fixed in #1."), 0, "--base", no_ledger)
    expect2("exit 2: --base that is not a commit", repo, g, 2, "--base", "no-such-ref")

    # Rule B: a placeholder PR number, any state, outside backticks.
    expect("B fail: #PRNUM in a grandfathered row", repo, "Fixed in #PRNUM (this PR); #1.", 1)
    expect("B fail: #TBD", repo, "Fixed in #TBD; #1.", 1)
    expect("B fail: #?", repo, "Fixed in #? ; #1.", 1)
    expect("B fail: #N in prose", repo, "cite it as #N; #1.", 1)
    expect("B pass: `#N` in backticks (the FND-317 / FND-134 shape)", repo,
           "the form `FIXED in #N` and `#NNN`; #1.", 0)
    expect2("B fail: #PRNUM in an open row", repo,
            row("FND-002", "open", "FIXED in #PRNUM later") + g, 1, *b)

    # Narrow C: with --pr, the cited number is this PR's or a merged one's.
    git(repo, "commit", "-q", "--allow-empty", "-m", "fix: a thing (#42)")
    git(repo, "commit", "-q", "--allow-empty", "-m", "Merge pull request #43 from x/y")
    for n, want, why in ((657, 0, "this PR"), (42, 0, "a squash subject"),
                         (43, 0, "a merge subject"), (999, 1, "neither")):
        expect2(f"C {'pass' if want == 0 else 'fail'}: --pr 657, cites #{n} ({why})", repo,
                row("FND-002", "fixed", f"**FIXED in #{n} (this PR).**") + g, want,
                *b, "--pr", "657")
    expect2("C pass: cites #999 without --pr (the local lane does not decide it)", repo,
            row("FND-002", "fixed", "**FIXED in #999 (this PR).**") + g, 0, *b)
    expect2("exit 2: --pr not a number", repo, g, 2, *b, "--pr", "PRNUM")

    # The default base, merge-base HEAD origin/main: every lane's path.
    expect2("exit 2: no --base and no origin/main", repo, g, 2)
    git(repo, "update-ref", "refs/remotes/origin/main", BASE)
    expect2("default base: grandfathered row passes", repo, g, 0)
    expect2("default base: new unphrased fixed row fails", repo,
            row("FND-002", "fixed", "Builds on #656.") + g, 1)
    git(repo, "update-ref", "-d", "refs/remotes/origin/main")

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

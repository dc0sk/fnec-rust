#!/usr/bin/env python3
"""Validate the shape of every row in `docs/project/findings-ledger.md`.

A ledger nobody checks decays into a list of things that *look* resolved. The
states carry obligations — a `deferred` finding needs an owner and somewhere it is
tracked, a `fixed` one needs the change that fixed it — and this enforces them, so
the build fails rather than the row quietly lying.

Deliberately shape-only. Whether a `fixed` row is *really* fixed is a question for
the change it cites, not for a grep; claiming to verify that here would be the kind
of check that passes forever without ever having looked.

One rule does reach into git, because it is a fact a grep can settle: a commit
the ledger cites must be reachable from HEAD. The PRs here are squash-merged, so
a branch commit's hash does not exist on `main` once the PR lands -- 16 rows
cited such hashes as "Fixed in <hash> (this PR)", resolvable only as dangling
objects in the clone that made them (FND-317). A cited hash must therefore be an
ancestor of HEAD, or carry its PR number right after it: `<hash> (#N)` or
`<hash> (PR #N, ...)`. HEAD rather than `main`, so a branch's own new commits
pass before it merges, and CI's PR merge commit has `main` as an ancestor.

Usage:
  scripts/check-findings-ledger.py          # exit 1 on any malformed row,
                                            # exit 2 if git history is unavailable
  scripts/check-findings-ledger.py --ledger PATH
"""

import re
import subprocess
import sys

LEDGER = "docs/project/findings-ledger.md"
TERMINAL = {"fixed", "deferred", "rejected"}
STATES = TERMINAL | {"open"}
ROW = re.compile(r"^\|\s*(FND-\d+)\s*\|([^|]*)\|([^|]*)\|([^|]*)\|([^|]*)\|\s*$")

# A hex token the ledger cites AS A COMMIT, by the phrasings the rows actually
# use: "fixed in <h>", "squashed to <h>", "main <h>" / "main at <h>", "minted at
# <h>", "commit <h>", "on `<h>`", "at `<h>`", and "(<h>" / "(`<h>`". Matching
# every 7-hex token instead would take in "SSH ed25519", allocation sizes
# ("of 2147483648 bytes") and float mantissas ("8.854187817e-12"); matching only
# hashes with a letter would miss "fixed in 2503790". A prose mention that is
# not a citation ("NONE of 720bf3e, b40c192 ...", FND-134) is not matched.
CITE = re.compile(
    r"(?:(?i:\b(?:in|to|at|main|commit|on))\s+|\()`?([0-9a-f]{7,40})(?![0-9A-Za-z_])"
)
# The citation carries its PR number when one follows it at once.
CARRIES_PR = re.compile(r"`?\s*\((?:PR\s*)?#\d+")


def git(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *args], capture_output=True, text=True)


def history_problem() -> str | None:
    """Why the ancestry rule cannot be decided here, or None if it can."""
    try:
        r = git("rev-parse", "--is-shallow-repository")
    except OSError as e:
        return f"cannot run git: {e}"
    if r.returncode != 0:
        return f"not a git checkout: {r.stderr.strip()}"
    if r.stdout.strip() == "true":
        # A shallow clone cannot tell "not an ancestor" from "not fetched", so
        # every old citation would fail. Refuse rather than skip: CI's docs job
        # checks out with fetch-depth: 0, so this never fires there, and a skip
        # would be a green that looked at nothing.
        return "shallow clone: run `git fetch --unshallow` (CI: fetch-depth: 0)"
    return None


def dangling_citations(lineno: int, fid: str, line: str) -> list[str]:
    out = []
    for m in CITE.finditer(line):
        h = m.group(1)
        if CARRIES_PR.match(line, m.end()):
            continue
        is_commit = git("cat-file", "-e", f"{h}^{{commit}}").returncode == 0
        if is_commit and git("merge-base", "--is-ancestor", h, "HEAD").returncode == 0:
            continue
        why = "not an ancestor of HEAD" if is_commit else "no such commit"
        out.append(
            f"{LEDGER}:{lineno}: {fid} cites commit {h} ({why}); cite the PR as "
            f"#N, or a commit on main followed by (#N)"
        )
    return out


def main() -> int:
    global LEDGER
    args = sys.argv[1:]
    if args[:1] == ["--ledger"] and len(args) == 2:
        LEDGER = args[1]
    elif args:
        print(f"usage: {sys.argv[0]} [--ledger PATH]", file=sys.stderr)
        return 2
    hist = history_problem()
    if hist:
        print(f"check-findings-ledger: {hist}", file=sys.stderr)
        return 2
    try:
        lines = open(LEDGER, encoding="utf-8").read().splitlines()
    except OSError as e:
        print(f"cannot read {LEDGER}: {e}", file=sys.stderr)
        return 1

    problems: list[str] = []
    seen: dict[str, int] = {}
    order: list[int] = []

    for lineno, line in enumerate(lines, 1):
        # Tolerant on the way IN, strict once inside. This was
        # `line.startswith("| FND-")`, which demanded exactly one space after the
        # leading pipe -- so a row that got the spacing wrong was skipped
        # ENTIRELY rather than reported, and the gate then announced a green
        # count that did not include it. Reproduced: `|FND-999| not-a-date |
        # bogus-state | planted | none |` gave EXIT 0 and "137 finding(s)" with
        # 138 rows present (FND-090).
        #
        # A checker's entry filter must never be stricter than the thing it
        # checks for, or the malformed cases select themselves out of the audit.
        if not re.match(r"^\|\s*FND-", line):
            continue
        m = ROW.match(line)
        if not m:
            problems.append(f"{LEDGER}:{lineno}: malformed row (expected 5 columns)")
            continue
        fid, found, state, finding, evidence = (g.strip() for g in m.groups())

        if fid in seen:
            problems.append(f"{LEDGER}:{lineno}: {fid} reused (first seen line {seen[fid]})")
        seen[fid] = lineno
        order.append(int(fid.split("-")[1]))
        problems.extend(dangling_citations(lineno, fid, line))

        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", found):
            problems.append(f"{LEDGER}:{lineno}: {fid} 'Found' must be an ISO date, got {found!r}")
        if state not in STATES:
            problems.append(
                f"{LEDGER}:{lineno}: {fid} state {state!r} is not one of {sorted(STATES)}"
            )
            continue
        if not finding:
            problems.append(f"{LEDGER}:{lineno}: {fid} has no finding text")

        # Each terminal state owes something specific.
        if state == "deferred" and "owner:" not in evidence.lower():
            problems.append(
                f"{LEDGER}:{lineno}: {fid} is deferred but names no owner "
                f"(evidence column must contain 'owner:')"
            )
        if state == "fixed" and not re.search(r"#\d+|[0-9a-f]{7,40}", evidence):
            problems.append(
                f"{LEDGER}:{lineno}: {fid} is fixed but cites no PR (#N) or commit"
            )
        if state == "rejected" and len(evidence) < 20:
            problems.append(
                f"{LEDGER}:{lineno}: {fid} is rejected but gives no rationale"
            )
        if state != "open" and not evidence:
            problems.append(f"{LEDGER}:{lineno}: {fid} is {state} with an empty evidence column")

    if not seen:
        problems.append(f"{LEDGER}: no FND- rows found; the ledger cannot be empty")
    if order != sorted(order, reverse=True):
        problems.append(f"{LEDGER}: rows must be newest-first (descending FND- number)")

    if problems:
        for p in problems:
            print(p, file=sys.stderr)
        return 1

    # The same filter, second copy -- and it had the same bug. Counting the open
    # rows with a stricter predicate than the one that validated them would let a
    # misspaced `open` row pass validation and then vanish from the count, which
    # is a quieter version of the same lie (FND-090). This runs only after
    # `problems` is empty, so every row here is already known well-formed.
    open_n = sum(1 for ln in lines if re.match(r"^\|\s*FND-", ln) and "| open |" in ln)
    print(f"findings ledger OK — {len(seen)} finding(s), {open_n} open")
    return 0


if __name__ == "__main__":
    sys.exit(main())

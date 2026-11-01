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

A row that turns `fixed` must name its fixing PR (FND-323). A `#N` anywhere in
the evidence satisfied the older rule, so FND-320's row, carrying `#656` (the PR
that added the ban, not the migration) beside a placeholder `FIXED in #PRNUM`,
passed `check-all.sh` locally on an uncommitted tree. So a row whose state is
`fixed` on disk and was not `fixed` at the base (or did not exist there) must
carry `fixed in #N` / `fixed by #N` / `fixed in PR #N` in its evidence column.
The 114 older `fixed` rows without that phrase are grandfathered by their state
at the base, not by a list; they still owe a `#N`. Three properties make this
hold in every lane:

- Push to main is a no-op for it, by design. The base is `merge-base HEAD
  origin/main`; on main that is HEAD itself, so 0 rows turned fixed. Branch
  protection is `strict`, so the tree main receives is the PR merge tree the PR's
  run checked; no ledger change reaches main unchecked. (Main has merge commits,
  so `HEAD~1` is no alternative base.) The base and the count are printed, so a
  0 is a statement, not silence.
- Staleness biases to refusals, not passes. A stale `origin/main` moves the base
  back, so more rows look newly fixed; a row cannot escape the rule that way.
- The comparison is the base blob against the file on disk, never `git diff
  BASE HEAD`: `check-all.sh` runs before a commit, and a placeholder that never
  reaches a commit must be caught here.

In PR CI, `--pr N` (this PR's number) also requires the number that rule names to
be N or a PR merged on HEAD: the workflow writes the row before the PR merges,
and a predicted number another issue or PR took would cite a stranger forever
(FND-134, FND-317). Without `--pr` that part is not decided; it is a tightening
only CI can do, as `check-version-bump-docs.sh` runs only on a pull request.

Usage:
  scripts/check-findings-ledger.py          # exit 1 on any malformed row,
                                            # exit 2 if git history or the base
                                            # is unavailable
  scripts/check-findings-ledger.py [--ledger PATH] [--base REF] [--pr N]

  --base REF  the base rule A compares row states against (default:
              `git merge-base HEAD origin/main`; neither -> exit 2)
  --pr N      this PR's number (CI passes it on a pull_request run)
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

# Rule A's phrase, on the evidence column only: the Finding column may say
# "fixed in #N" about something else.
FIX_PHRASE = re.compile(r"\bfixed\s+(?:in|by)\s+(?:PR\s*)?#(\d+)", re.I)
# Rule B: a placeholder where a PR number belongs (`#PRNUM`, `#TBD`, `#XXX`,
# `#N`, `#?`), checked with backticked spans removed, because the ledger quotes
# the citation *form* in backticks (FND-317's row says `#N`, FND-134's `#NNN`).
# The loophole -- a backticked `#PRNUM` passes B -- is closed by rule A, which
# still demands a real `fixed in #<digits>` on any row that turns fixed.
PLACEHOLDER = re.compile(r"#(?:[A-Z][A-Z0-9]*\b|\?+)")
BACKTICKED = re.compile(r"`[^`]*`")
# PR numbers in `git log --format=%s` subjects: squash `... (#N)` and merge
# commits `Merge pull request #N ...`.
SQUASH_SUBJECT = re.compile(r"\(#(\d+)\)\s*$")
MERGE_SUBJECT = re.compile(r"^Merge pull request #(\d+)")


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


def resolve_base(base_arg: str | None) -> tuple[str, str] | None:
    """(commit, how) for rule A's base, or None (the caller exits 2).

    `--base REF` if given, else `merge-base HEAD origin/main` -- the same rule
    `check-all.sh` uses for its other base-diff gates. Never a skip: with no base
    rule A cannot be decided, and a skip would be a green that looked at nothing.
    """
    if base_arg is not None:
        r = git("rev-parse", "--verify", "--quiet", f"{base_arg}^{{commit}}")
        if r.returncode != 0:
            print(f"check-findings-ledger: --base {base_arg!r} is not a commit", file=sys.stderr)
            return None
        return r.stdout.strip(), "--base"
    r = git("merge-base", "HEAD", "origin/main")
    if r.returncode != 0:
        print(
            "check-findings-ledger: no origin/main and no --base REF; rule A cannot be decided",
            file=sys.stderr,
        )
        return None
    return r.stdout.strip(), "merge-base origin/main"


def base_states(base: str) -> dict[str, str]:
    """{FND-id: state} of the ledger at `base`; {} if the file is absent there.

    The same ROW regex and the same tolerant entry filter as the current parse
    (FND-090: the base parse must not be stricter than the current one). An
    absent blob makes every fixed row new, which is strict, and is what a
    brand-new ledger should get.
    """
    r = git("show", f"{base}:./{LEDGER}")
    if r.returncode != 0:
        return {}
    out: dict[str, str] = {}
    for line in r.stdout.splitlines():
        if not re.match(r"^\|\s*FND-", line):
            continue
        m = ROW.match(line)
        if m:
            out[m.group(1).strip()] = m.group(3).strip()
    return out


def merged_prs() -> set[int]:
    """PR numbers merged on HEAD, from squash and merge-commit subjects.

    HEAD rather than `origin/main`, as for the ancestry rule: CI's PR merge
    commit has main as an ancestor.
    """
    r = git("log", "--format=%s", "HEAD")
    out: set[int] = set()
    for subj in r.stdout.splitlines():
        for rx in (SQUASH_SUBJECT, MERGE_SUBJECT):
            m = rx.search(subj)
            if m:
                out.add(int(m.group(1)))
    return out


def parse_args(argv: list[str]) -> dict | None:
    opts: dict = {"base": None, "pr": None}
    it = iter(argv)
    for a in it:
        val = next(it, None)
        if a not in ("--ledger", "--base", "--pr") or val is None:
            return None
        if a == "--pr":
            if not re.fullmatch(r"[1-9]\d*", val):
                return None
            opts["pr"] = int(val)
        else:
            opts[a[2:]] = val
    return opts


def main() -> int:
    global LEDGER
    opts = parse_args(sys.argv[1:])
    if opts is None:
        print(
            f"usage: {sys.argv[0]} [--ledger PATH] [--base REF] [--pr N]", file=sys.stderr
        )
        return 2
    if opts.get("ledger"):
        LEDGER = opts["ledger"]
    hist = history_problem()
    if hist:
        print(f"check-findings-ledger: {hist}", file=sys.stderr)
        return 2
    resolved = resolve_base(opts["base"])
    if resolved is None:
        return 2
    base, how = resolved
    print(f"rule A: base {base[:7]} ({how})")
    before = base_states(base)
    pr = opts["pr"]
    merged = merged_prs() if pr is not None else set()
    turned = 0
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
        # A PR number, not any hex token: a hash on main may be the base a fix
        # was measured on (FND-310's row passed on "main a6b7ffb"), and a branch
        # hash dies at the squash (FND-317). Shape-only for the grandfathered
        # rows ("see #N" passes); a row turning fixed owes rule A's phrase above.
        if state == "fixed" and not re.search(r"#\d+", evidence):
            problems.append(
                f"{LEDGER}:{lineno}: {fid} is fixed but cites no PR (#N); a commit "
                f"hash is not a citation of the fix (squash-merge, FND-317), and a "
                f"hash on main may be a measurement base (FND-310)"
            )
        # Rule B, every row whatever its state, the whole row.
        for tok in PLACEHOLDER.findall(BACKTICKED.sub("", line)):
            problems.append(
                f"{LEDGER}:{lineno}: {fid} contains a placeholder PR number {tok!r}; a "
                f"row may not be committed before its PR number is known — open the "
                f"PR (draft is fine), then write FIXED in #N"
            )
        # Rule A: a row that turns fixed in this change names its fixing PR.
        if state == "fixed" and before.get(fid) != "fixed":
            turned += 1
            cited = [int(n) for n in FIX_PHRASE.findall(evidence)]
            if not cited:
                problems.append(
                    f"{LEDGER}:{lineno}: {fid} turned fixed in this change but names no "
                    f'fixing PR: write "FIXED in #N" with the PR that fixed it (any '
                    f"other #N in the row is not that)"
                )
            # In PR CI only: the number must be this PR's or a merged one's. A
            # stacked PR whose row cites its still-open parent is refused until the
            # parent merges; `strict` branch protection re-runs its CI then. That is
            # the intended order, not a case to exempt.
            for n in cited if pr is not None else ():
                if n != pr and n not in merged:
                    problems.append(
                        f"{LEDGER}:{lineno}: {fid} cites #{n} as the fix, but #{n} is "
                        f"neither this PR (#{pr}) nor a PR merged on main; a predicted "
                        f"number that another issue or PR took (FND-134, FND-317)"
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

    print(f"rule A: {turned} row{'' if turned == 1 else 's'} turned fixed, {turned} checked")
    if pr is None:
        print("rule A: PR number not given; the cited number is checked in CI only")
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

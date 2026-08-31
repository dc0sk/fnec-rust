#!/usr/bin/env python3
"""Validate the shape of every row in `docs/project/findings-ledger.md`.

A ledger nobody checks decays into a list of things that *look* resolved. The
states carry obligations — a `deferred` finding needs an owner and somewhere it is
tracked, a `fixed` one needs the change that fixed it — and this enforces them, so
the build fails rather than the row quietly lying.

Deliberately shape-only. Whether a `fixed` row is *really* fixed is a question for
the change it cites, not for a grep; claiming to verify that here would be the kind
of check that passes forever without ever having looked.

Usage:
  scripts/check-findings-ledger.py          # exit 1 on any malformed row
"""

import re
import sys

LEDGER = "docs/project/findings-ledger.md"
TERMINAL = {"fixed", "deferred", "rejected"}
STATES = TERMINAL | {"open"}
ROW = re.compile(r"^\|\s*(FND-\d+)\s*\|([^|]*)\|([^|]*)\|([^|]*)\|([^|]*)\|\s*$")


def main() -> int:
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

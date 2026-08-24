#!/usr/bin/env python3
"""Keep `docs/project/path-inventory.md` from rotting into fiction.

Two ways an inventory decays into something that reads as coverage while proving
nothing: it cites a test that has since been renamed or deleted, or it marks a path
as a gap without linking anywhere, so the gap is recorded and then forgotten.

This checks references only. Whether a cited test genuinely exercises its path is a
question for the test, not for a grep — a checker claiming otherwise would pass
forever without having looked.

Usage:
  scripts/check-path-inventory.py        # exit 1 on a dangling reference
"""

import re
import subprocess
import sys
from pathlib import Path

INVENTORY = Path("docs/project/path-inventory.md")
LEDGER = Path("docs/project/findings-ledger.md")

# `path/to/file.rs` or `path/to/file.py` mentioned in a table row.
FILE_RE = re.compile(r"`((?:apps|crates|bindings|scripts)/[\w./-]+\.(?:rs|py))`")
# A Rust symbol in backticks: snake_case with at least one underscore, e.g.
# `every_gui_solve_path_...` or `solve_deck_at_frequency_with_exec`.
#
# This used to require 15+ characters, which let a 10-character INVENTED symbol
# (`solve_task`, a function that never existed) sit in this file and the findings
# ledger through two reviews. Length is not what makes a name checkable — having an
# underscore is, because that is what distinguishes an identifier from prose.
SYMBOL_RE = re.compile(r"`([a-z][a-z0-9]*(?:_[a-z0-9]+)+)`")
# Words that look like identifiers but are prose or file suffixes, not symbols.
SYMBOL_ALLOWLIST = {"nec_solver", "nec_worker", "nec_model", "nec_parser", "fnec_py"}
FND_RE = re.compile(r"FND-\d+")


def read_ledger_states() -> dict[str, str]:
    """Map every `FND-NNN` in the ledger to its state.

    Only the ledger's own table rows count, so a finding *mentioned* in another
    row's prose does not silently become an owner.
    """
    if not LEDGER.exists():
        return {}
    states: dict[str, str] = {}
    for line in LEDGER.read_text(encoding="utf-8").splitlines():
        cells = [c.strip() for c in line.split("|")]
        # "| FND-001 | date | state | ... |" -> ['', 'FND-001', date, state, ...]
        if len(cells) >= 4 and FND_RE.fullmatch(cells[1] or ""):
            states[cells[1]] = cells[3]
    return states


def main() -> int:
    if not INVENTORY.exists():
        print(f"{INVENTORY} is missing", file=sys.stderr)
        return 1
    text = INVENTORY.read_text(encoding="utf-8")
    ledger_state = read_ledger_states()
    ledger_ids = set(ledger_state)

    problems: list[str] = []
    rows = 0
    checked_files = 0
    checked_tests = 0

    for lineno, line in enumerate(text.splitlines(), 1):
        if not re.match(r"^\|\s*\d+\s*\|", line):
            continue
        rows += 1

        for rel in FILE_RE.findall(line):
            checked_files += 1
            if not Path(rel).exists():
                problems.append(f"{INVENTORY}:{lineno}: cites {rel}, which does not exist")

        # Every gap row must point at a finding that is really in the ledger.
        if re.search(r"\*\*NO\*\*", line):
            ids = FND_RE.findall(line)
            if not ids:
                problems.append(
                    f"{INVENTORY}:{lineno}: marked as a gap but cites no FND- finding"
                )
            for fid in ids:
                if fid not in ledger_ids:
                    problems.append(
                        f"{INVENTORY}:{lineno}: cites {fid}, which is not in {LEDGER}"
                    )
                # An OPEN gap owned by a CLOSED finding is exactly the decay this
                # file exists to stop: the row still says "NO", the ledger says
                # the work is done, and nobody owns the difference. This check
                # was added after #396 closed FND-031 and left two gap rows
                # pointing at it — the checker passed, because it only asked
                # whether the link resolved.
                elif ledger_state[fid] not in ("open", "deferred"):
                    problems.append(
                        f"{INVENTORY}:{lineno}: cites {fid}, which is "
                        f"'{ledger_state[fid]}' — a gap needs an owner that is "
                        f"still open or deferred"
                    )

        # Every referenced symbol must exist somewhere in the tree. A name that
        # resolves nowhere is either a typo or invented, and both read as evidence.
        for name in SYMBOL_RE.findall(line):
            if name in SYMBOL_ALLOWLIST or "/" in name:
                continue
            checked_tests += 1
            # Three things this needs, each learned by watching it pass when it
            # should not have:
            #   -w          whole-word, or a truncated name resolves to the real
            #               one it is a prefix of.
            #   :!docs/     or a name resolves to its own mention in the very file
            #               being checked.
            #   :!scripts/  or it resolves to the EXAMPLES IN THIS COMMENT — the
            #               first draft of this check was satisfied by its own
            #               explanatory text.
            # A symbol must be found in the shipped source tree, nowhere else.
            found = (
                subprocess.run(
                    ["git", "grep", "-qw", name, "--", ":!docs/", ":!scripts/"],
                    capture_output=True,
                ).returncode
                == 0
            )
            if not found:
                problems.append(
                    f"{INVENTORY}:{lineno}: cites `{name}`, which is nowhere in the tree"
                )

    if rows == 0:
        problems.append(f"{INVENTORY}: no path rows found; the inventory cannot be empty")

    if problems:
        for p in problems:
            print(p, file=sys.stderr)
        return 1
    print(
        f"path inventory OK — {rows} path(s), "
        f"{checked_files} file reference(s) and {checked_tests} symbol(s) resolve"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

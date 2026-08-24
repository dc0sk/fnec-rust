#!/usr/bin/env python3
"""Fail if a changelog release section repeats a `###` heading.

Entries are added one PR at a time, and each prepends its own heading, so an
`[Unreleased]` section drifts into `### Fixed` three times over. Keep a Changelog
1.1.0 wants one heading per type per release; nobody notices the drift until
release day, when it has to be untangled by hand — twice now.

Checks the shape of *unreleased and current* sections only: historical sections
predate the convention and are left as written.
"""

import collections
import re
import sys
from pathlib import Path

CHANGELOG = Path("docs/changelog.md")
# Keep a Changelog's six, plus the one this project has long used for doc-only work.
ALLOWED = {"Added", "Changed", "Deprecated", "Removed", "Fixed", "Security", "Docs"}
# Sections written before the convention was adopted; not rewritten retroactively.
GRANDFATHERED_BEFORE = "0.14.0"


def _version(name: str) -> tuple[int, ...]:
    """Parse `1.2.3` into `(1, 2, 3)`; anything unparseable sorts last so it is checked."""
    try:
        return tuple(int(x) for x in name.split("."))
    except ValueError:
        return (9999,)


def main() -> int:
    text = CHANGELOG.read_text(encoding="utf-8")
    # Split on release headings: "## [x.y.z] …" or "## [Unreleased]".
    sections = re.split(r"^## \[([^\]]+)\]", text, flags=re.M)
    problems: list[str] = []
    checked = 0

    for i in range(1, len(sections), 2):
        name, body = sections[i], sections[i + 1]
        if name != "Unreleased":
            # Only the current release line onwards is held to the convention.
            # Compare as version tuples — string order puts "0.4.0" after "0.14.0".
            if _version(name) < _version(GRANDFATHERED_BEFORE):
                continue
        checked += 1
        heads = re.findall(r"^### (.+)$", body, flags=re.M)
        for head, n in collections.Counter(heads).items():
            if n > 1:
                problems.append(
                    f"{CHANGELOG}: [{name}] repeats '### {head}' {n} times "
                    f"— merge the entries under one heading"
                )
            if head not in ALLOWED:
                problems.append(
                    f"{CHANGELOG}: [{name}] uses '### {head}', which is not one of "
                    f"{sorted(ALLOWED)}"
                )

    if problems:
        for p in problems:
            print(p, file=sys.stderr)
        return 1
    print(f"changelog headings OK — {checked} section(s) checked")
    return 0


if __name__ == "__main__":
    sys.exit(main())

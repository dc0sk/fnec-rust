#!/usr/bin/env python3
"""Fail if the changelog's `[Unreleased]` section has any content.

Run by the Mint release tag workflow against the commit it is about to tag, not
by CI: between releases `[Unreleased]` is where every PR's entry goes. At a mint
it must be empty, because the workflow tags main's TIP, and an entry still under
`[Unreleased]` there is a change the tag carries that its release section does
not list (docs/release-process.md step 1). That rule was written down and missed
three times — 0.19.0 (#482), 0.20.0 (#510) and 0.23.0 (#603), each fixed by a docs
PR after the tag — because PRs merged after the release PR, onto the tip the mint
then tagged.

A missing `[Unreleased]` heading passes: nothing is unreleased.

Run: python3 scripts/check-changelog-unreleased-empty.py [CHANGELOG]
"""

import re
import sys
from pathlib import Path

CHANGELOG = Path("docs/changelog.md")


def unreleased_lines(text: str) -> list[str]:
    """The non-blank lines between `## [Unreleased]` and the next `## ` heading."""
    m = re.search(r"^## \[Unreleased\][^\n]*\n(.*?)(?=^## |\Z)", text, flags=re.M | re.S)
    if m is None:
        return []
    return [line for line in m.group(1).split("\n") if line.strip()]


def main() -> int:
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else CHANGELOG
    lines = unreleased_lines(path.read_text(encoding="utf-8"))
    if lines:
        print(
            f"{path}: [Unreleased] is not empty at the commit being tagged — the tag "
            f"would carry changes its release section does not list. Move these "
            f"entries under the release heading (docs/release-process.md step 1) "
            f"and mint again:",
            file=sys.stderr,
        )
        for line in lines[:10]:
            print(f"  {line}", file=sys.stderr)
        if len(lines) > 10:
            print(f"  … {len(lines) - 10} more line(s)", file=sys.stderr)
        return 1
    print(f"{path}: [Unreleased] is empty")
    return 0


if __name__ == "__main__":
    sys.exit(main())

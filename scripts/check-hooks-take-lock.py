#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Every repo hook that runs cargo takes the host-wide build lock first (FND-261).

    scripts/check-hooks-take-lock.py

A git hook's cargo run is as heavy as a gate's: the pre-commit hook's workspace
clippy builds the whole workspace. Without the lock it ran beside another
project's suite, which is how a gate here was OOM-killed three times on
2026-09-25. The check: in each file under .githooks/, a line sourcing
scripts/host-build-lock.sh comes before the first line that runs cargo.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCK = re.compile(r"^\s*(source|\.)\s+.*host-build-lock\.sh")
CARGO = re.compile(r"^\s*cargo\s")


def main():
    bad = []
    hooks = sorted(p for p in (ROOT / ".githooks").iterdir() if p.is_file())
    for hook in hooks:
        locked = False
        for n, line in enumerate(hook.read_text().splitlines(), 1):
            if LOCK.search(line):
                locked = True
            elif CARGO.search(line) and not locked:
                bad.append(f"{hook.relative_to(ROOT)}:{n}: runs cargo before taking the host build lock")
                break
    if bad:
        print("\n".join(bad))
        sys.exit(1)
    print(f"hooks take the build lock — {len(hooks)} hook(s) checked")


if __name__ == "__main__":
    main()

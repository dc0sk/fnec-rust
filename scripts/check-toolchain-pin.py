#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Every toolchain this repo lints and tests with is the one in rust-toolchain.toml.

FND-149: CI named `dtolnay/rust-toolchain@1.97.1` in seven places and nothing
else pinned anything, so the pre-commit hook linted with whatever clippy the
host had. One minor version ahead, four pre-existing sites became hard errors
and every commit needed `--no-verify` — a gate that had to be bypassed to commit.

Checks:
  1. every `dtolnay/rust-toolchain@<ref>` in .github/workflows names the pin
     (a floating `@stable` is allowed only in the workflows listed below, which
     gate nothing);
  2. with --local, the `rustc` on PATH is the pin.

Usage: scripts/check-toolchain-pin.py [--local]
"""

import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# Workflows that do not gate a merge, so a rolling toolchain cannot turn them red
# for no reason anyone must act on.
UNGATED = {"benchmark-dashboard.yml"}
USES = re.compile(r"uses:\s*dtolnay/rust-toolchain@([\w.\-]+)")


def main() -> int:
    pin = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    problems = []
    for wf in sorted((ROOT / ".github" / "workflows").glob("*.yml")):
        for n, line in enumerate(wf.read_text().splitlines(), 1):
            m = USES.search(line)
            if m and m.group(1) != pin and wf.name not in UNGATED:
                problems.append(f"{wf.relative_to(ROOT)}:{n}: @{m.group(1)}, pin is {pin}")
    if "--local" in sys.argv:
        out = subprocess.run(["rustc", "--version"], capture_output=True, text=True, check=False)
        have = out.stdout.split()[1] if out.returncode == 0 and out.stdout else "(no rustc)"
        if have != pin:
            problems.append(
                f"local rustc is {have}, pin is {pin}: the hooks would lint with a "
                "clippy CI does not run"
            )
    if problems:
        print("toolchain pin mismatch:", *problems, sep="\n  ", file=sys.stderr)
        return 1
    print(f"toolchain pin OK — {pin}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Known-pass / known-fail cases for `check-scratch-paths.py` (FND-320).

Each case builds a scratch tree with the owner, a test file and an allowlist,
and runs the checker on it with `--root`. A new file calling `temp_dir()` must
fail; the same file allowlisted must pass; a stale entry, an entry without a
reason and a duplicate entry must fail; a mention in a comment, or a call in the
owner, must pass; and a missing tests directory must exit 2, not 0.

Run: python3 scripts/test-check-scratch-paths.py
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

CHECKER = Path(__file__).resolve().parent / "check-scratch-paths.py"
CALL = 'let p = std::env::temp_dir().join("x.nec");\n'
failures: list[str] = []
ran = 0


def case(name: str, files: dict[str, str], allow: str, want: int, owner: bool = True) -> None:
    global ran
    ran += 1
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        if owner:
            (root / "apps/nec-cli/tests/common").mkdir(parents=True)
            (root / "apps/nec-cli/tests/common/mod.rs").write_text("pub fn f() { " + CALL + "}\n")
        for rel, body in files.items():
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(body)
        (root / "scripts").mkdir()
        (root / "scripts/scratch-paths-allowlist.txt").write_text(allow)
        r = subprocess.run([sys.executable, str(CHECKER), "--root", d],
                           capture_output=True, text=True)
        if r.returncode != want:
            failures.append(f"{name}: exit {r.returncode}, want {want}\n{r.stdout}{r.stderr}")


A = "apps/nec-cli/tests/a.rs"
case("fail: a new file calls temp_dir()", {A: CALL}, "", 1)
case("fail: the imported spelling", {A: "use std::env::temp_dir;\nlet p = temp_dir();\n"}, "", 1)
case("fail: a nested file", {"apps/nec-cli/tests/sub/b.rs": CALL}, "", 1)
case("pass: the same file allowlisted", {A: CALL}, f"{A}  # reason\n", 0)
case("fail: a stale entry", {A: "fn f() {}\n"}, f"{A}  # reason\n", 1)
case("fail: an entry whose file is gone", {}, f"{A}  # reason\n", 1)
case("fail: an entry without a reason", {A: CALL}, f"{A}\n", 1)
case("fail: a duplicate entry", {A: CALL}, f"{A}  # r\n{A}  # r\n", 1)
case("pass: a mention in a comment", {A: "// never call temp_dir() here\nfn f() {}\n"}, "", 0)
case("pass: the owner's own call", {}, "# comments only\n", 0)
case("pass: a string mentioning it is a call-free literal", {A: 'let s = "x"; // temp_dir()\n'}, "", 0)
case("exit 2: no tests directory", {}, "", 2, owner=False)

if failures:
    for f in failures:
        print(f"FAIL {f}", file=sys.stderr)
    print(f"{len(failures)} of {ran} case(s) failed", file=sys.stderr)
    sys.exit(1)
print(f"check-scratch-paths self-test OK — {ran} case(s)")

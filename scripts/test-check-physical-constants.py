#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Known-pass / known-fail cases for `check-physical-constants.py`.

A literal-ban gate that matches nothing passes every tree, so each spelling a
copy has actually been written in on this repo (FND-297 b) is planted in a scratch
tree here and must fail it; the exemptions (the owner, `tests/`, comments, `target/`)
must pass; and a tree without the owner must exit 2, not 0.

Run: python3 scripts/test-check-physical-constants.py
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).parent
CHECKER = HERE / "check-physical-constants.py"
OWNER = "crates/nec_model/src/constants.rs"

failures: list[str] = []
ran = 0


def run(files: dict[str, str], owner: bool = True) -> int:
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        if owner:
            files = {OWNER: "pub const C0: f64 = 299_792_458.0;\n", **files}
        for rel, text in files.items():
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
        r = subprocess.run(
            [sys.executable, str(CHECKER), "--root", str(root)], capture_output=True, text=True
        )
        return r.returncode


def expect(name: str, files: dict[str, str], want: int, owner: bool = True) -> None:
    global ran
    ran += 1
    got = run(files, owner)
    if got != want:
        failures.append(f"{name}: exit {got}, want {want}")


SRC = "crates/nec_solver/src/x.rs"

# Every spelling the tree carried before the move, one per case (known-fail).
for name, line in [
    ("c0 with separators", "const C0: f64 = 299_792_458.0; // m/s"),
    ("c0 inline", "let k = 2.0 * std::f64::consts::PI * freq_hz / 299_792_458.0;"),
    ("c0 without separators", "let c = 299792458.0;"),
    ("c0 as 2.99792e8", "let c = 2.99792458e8;"),
    ("c0 as 3e8", "let c = 3e8;"),
    ("c0 as 3.0e8", "let c = 3.0e8;"),
    ("mu0 4·π·1e-7", "const MU0: f64 = 4.0 * std::f64::consts::PI * 1e-7;"),
    ("mu0 4·PI·1e-7 (imported PI)", "const MU0: f64 = 4.0 * PI * 1e-7;"),
    ("mu0 4e-7·π", "const MU0: f64 = 4.0e-7 * std::f64::consts::PI;"),
    ("mu0 π·4e-7", "let mu = PI * 4e-7;"),
    ("mu0 1.2566e-6", "let mu = 1.25663706e-6;"),
    ("eta0 inline", "const ETA0: f64 = 4.0e-7 * std::f64::consts::PI * 299_792_458.0;"),
    ("eta0 376.73", "let eta = 376.730_313_668;"),
    ("eta0 120π", "let eta = 120.0 * std::f64::consts::PI;"),
    ("eps0 2018", "const EPS0: f64 = 8.854_187_812_8e-12;"),
    ("eps0 1/(mu0 c^2)", "const EPS0: f64 = 8.854_187_817e-12; // F/m"),
    ("nec2c 59.96", "let x = sig * lambda * 59.96;"),
]:
    expect(f"fail: {name}", {SRC: line + "\n"}, 1)

# The same literal in each scanned top-level directory (known-fail).
for top in ("apps/nec-cli/src/a.rs", "apps/nec-cli/examples/b.rs", "bindings/fnec_py/src/lib.rs",
            "crates/nec_accel/src/c.rs"):
    expect(f"fail: scanned {top}", {top: "let c = 299_792_458.0;\n"}, 1)
# An in-file test module is not exempt.
expect("fail: #[cfg(test)] module", {SRC: "#[cfg(test)]\nmod t {\n    const K: f64 = 1.0 / 299_792_458.0;\n}\n"}, 1)
# Only a crate's own `tests/` directory is exempt, not a `tests` module under src/.
expect("fail: src/tests/ module", {"crates/nec_solver/src/tests/m.rs": "let c = 299_792_458.0;\n"}, 1)
# A literal after a string containing `//` is still code.
expect("fail: after a URL string", {SRC: 'let u = "https://x"; let c = 299_792_458.0;\n'}, 1)

# Known-pass.
expect("pass: clean tree", {SRC: "use crate::constants::C0;\nlet k = 2.0 * PI * f / C0;\n"}, 0)
expect("pass: owner only", {}, 0)
expect("pass: line comment", {SRC: "let x = 1.0; // nec2c's 59.96 for 1/(2π·c·ε0), c = 299 792 458\n"}, 0)
expect("pass: doc comment", {SRC: "/// `εc = εr − j·sig·λ·59.96` (nec2c main.c)\nfn f() {}\n"}, 0)
expect("pass: integration test", {"crates/nec_solver/tests/t.rs": "const C0: f64 = 299_792_458.0;\n"}, 0)
expect("pass: app integration test", {"apps/nec-cli/tests/t.rs": "let eta0 = 376.730_313_668;\n"}, 0)
expect("pass: target dir", {"crates/target/x.rs": "const C0: f64 = 299_792_458.0;\n"}, 0)
expect("pass: outside scanned dirs", {"studies/x.rs": "const C0: f64 = 299_792_458.0;\n"}, 0)
expect("pass: unrelated 1e-7", {SRC: "let l = 1e-7; let c = 4.0 * 1e-7;\n"}, 0)
expect("pass: 3e8 inside a longer number", {SRC: "let x = 13e8; let y = 3e80;\n"}, 0)

# A tree with no owner: the gate scanned the wrong root and must not pass.
expect("exit 2: no owner", {SRC: "let k = 1.0;\n"}, 2, owner=False)

if failures:
    print(f"test-check-physical-constants: {len(failures)} of {ran} cases failed:")
    for f in failures:
        print(f"  {f}")
    sys.exit(1)
print(f"test-check-physical-constants: {ran} cases passed")

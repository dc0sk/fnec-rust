#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""nec2c's mesh series for a corpus deck, as a converged reference records it (FND-255).

    scripts/nec2c-converged-series.py DECK [--ek] [--to S]

Runs nec2c on DECK meshed 1, 3, 9, ... S times finer (`nec2c-fine-mesh.py`; S a
power of 3, default 9) and prints the feedpoint impedances as JSON:
`{"kernel": "thin"|"extended", "1x": [[R, X], ...], "3x": ..., ...}`, one [R, X]
per feedpoint row in nec2c's order (frequency, then source). With `--ek` the deck
gets `EK 0` after `GE` and every multiplier runs on the extended kernel, which a
mesh below the thin-wire limit needs; the series then also carries `9x_thin`, the
9x point on the thin kernel, so a test can check the two agree where both apply.

The deck is adapted for nec2c the way the corpus captures always were: leading
`CE` comment lines become `CM` (nec2c ends the comments at the first `CE`), and
`XQ` is added before `EN` when the deck has no `XQ` or `RP` (nec2c solves nothing
at `EN` without one). nec2c's runs go in a scratch directory with short names:
it aborts on a long input path.
"""

import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROW = re.compile(r"^ +\d+ +\d+ ")


def fail(msg):
    print(f"nec2c-converged-series: {msg}", file=sys.stderr)
    sys.exit(1)


def adapt(text, ek):
    lines = text.split("\n")
    head = 0
    while head < len(lines) and lines[head].startswith(("CM", "CE")):
        head += 1
    last_ce = max((k for k in range(head) if lines[k].startswith("CE")), default=None)
    out = []
    for k, line in enumerate(lines):
        if k < head and line.startswith("CE") and k != last_ce:
            line = "CM" + line[2:]
        if line.startswith("EN") and not any(c.startswith(("XQ", "RP")) for c in out):
            out.append("XQ")
        out.append(line)
        if ek and line.startswith("GE"):
            out.append("EK 0")
    return "\n".join(out)


def impedances(report):
    rows, lines, i = [], report.split("\n"), 0
    while i < len(lines):
        if "ANTENNA INPUT PARAMETERS" not in lines[i]:
            i += 1
            continue
        i += 1
        while i < len(lines) and not ROW.match(lines[i]):
            i += 1
        while i < len(lines) and ROW.match(lines[i]):
            f = lines[i].split()
            rows.append([float(f[6]), float(f[7])])
            i += 1
    return rows


def run(deck, s, scratch):
    fine = subprocess.run(
        [sys.executable, str(HERE / "nec2c-fine-mesh.py"), str(deck), str(s)],
        capture_output=True, text=True,
    )
    if fine.returncode:
        fail(f"{s}x: {fine.stderr.strip()}")
    inp, out = scratch / f"m{s}.nec", scratch / f"m{s}.out"
    inp.write_text(fine.stdout)
    r = subprocess.run(["nec2c", "-i", str(inp), "-o", str(out)], capture_output=True, text=True)
    if r.returncode or r.stderr.strip():
        fail(f"nec2c at {s}x: {r.stderr.strip() or r.returncode}")
    rows = impedances(out.read_text())
    if not rows:
        fail(f"nec2c at {s}x printed no feedpoint impedance")
    return rows


def main():
    args = sys.argv[1:]
    ek = "--ek" in args
    to = 9
    if "--to" in args:
        to = int(args[args.index("--to") + 1])
        del args[args.index("--to"): args.index("--to") + 2]
    args = [a for a in args if a != "--ek"]
    if len(args) != 1:
        fail("usage: nec2c-converged-series.py DECK [--ek] [--to S]")
    multipliers = [1]
    while multipliers[-1] < to:
        multipliers.append(multipliers[-1] * 3)
    if multipliers[-1] != to or to < 9:
        fail(f"--to {to}: must be a power of 3, at least 9 (three points)")
    text = Path(args[0]).read_text()
    series = {"kernel": "extended" if ek else "thin"}
    with tempfile.TemporaryDirectory(prefix="n2c") as tmp:
        scratch = Path(tmp)
        for variant, use_ek in ((("main", ek),) + ((("thin", False),) if ek else ())):
            deck = scratch / f"{variant}.nec"
            deck.write_text(adapt(text, use_ek))
            for s in multipliers if variant == "main" else [9]:
                series[f"{s}x" if variant == "main" else "9x_thin"] = run(deck, s, scratch)
    counts = {len(v) for k, v in series.items() if k != "kernel"}
    if len(counts) != 1:
        fail(f"feedpoint row counts differ across the series: {counts}")
    print(json.dumps(series))


if __name__ == "__main__":
    main()

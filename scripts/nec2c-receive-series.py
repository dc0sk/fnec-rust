#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""nec2c's plane-wave receive currents for a deck, meshed 1, 3, 9, ... times finer (FND-265).

    scripts/nec2c-receive-series.py DECK S [S ...]

Every `GW` gets S times its segments, an `LD` on one segment k moves to S·k − (S−1)/2
(the same point), and the currents are read at each original segment k at that
same point. Every multiplier runs on nec2c's extended kernel (`EK 0`), whose limit
(segment/radius > 2) is asserted. Prints JSON:

    {"kernel": "extended",
     "S": {"peak": |I| max over the whole table, "I": [[re, im], ...],
           "corner_ratio": smallest segment/radius on a wire meeting another at an angle,
           "corner_below_10": true when that is under 10}, ...}

`corner_below_10` marks a multiplier where nec2c's extended kernel may not carry
its regime: at an inverted-L's corner of 8–9 radii a segment or fewer, the stem
current shifts uniformly (0.12 % of peak at 21 × 27, 1 mm), the same in free space
as the doubled structure, while a 0.2 mm control stays smooth to 27× and jumps at
81× (8.8 radii; a trace at 8.1, none at 15.9). A T's junction does not show it at
5.3 radii. A flagged point is a
reference only with a thinner-wire control showing the series' shape unchanged.

Refuses: an `EX` other than a plane wave (EX 1), an `LD` over more than one segment
or on tag 0, `GM`/`GX`/`GR` (they renumber). nec2c runs in a scratch directory with
short names: it aborts on a long input path.
"""

import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROW = re.compile(r"^ +\d+ +\d+ +-?\d+\.\d{4} ")


def fail(msg):
    print(f"nec2c-receive-series: {msg}", file=sys.stderr)
    sys.exit(1)


def wires(text):
    out = []
    for line in text.split("\n"):
        f = line.split()
        if f and f[0] == "GW":
            n = int(f[2])
            a, b, r = tuple(map(float, f[3:6])), tuple(map(float, f[6:9])), float(f[9])
            out.append((n, a, b, r))
    return out


def corner_ratio(ws, s):
    """Smallest segment/radius on a wire whose end meets another wire at an angle."""
    def unit(a, b):
        d = [b[i] - a[i] for i in range(3)]
        n = sum(x * x for x in d) ** 0.5
        return [x / n for x in d], n

    worst = None
    for i, (n, a, b, r) in enumerate(ws):
        u, length = unit(a, b)
        for j, (_, c, d, _) in enumerate(ws):
            if i == j:
                continue
            v, _ = unit(c, d)
            meets = any(sum((p[k] - q[k]) ** 2 for k in range(3)) < 1e-12 for p in (a, b) for q in (c, d))
            collinear = abs(abs(sum(u[k] * v[k] for k in range(3))) - 1) < 1e-9
            if meets and not collinear:
                ratio = length / (n * s) / r
                worst = ratio if worst is None else min(worst, ratio)
    return worst


def remesh(text, s):
    out, m = [], (lambda k: str(s * int(k) - (s - 1) // 2))
    for line in text.split("\n"):
        f = line.split()
        if not f:
            out.append(line)
            continue
        if f[0] in ("GM", "GX", "GR"):
            fail(f"{f[0]} renumbers segments; not mapped")
        if f[0] == "GW":
            f[2] = str(int(f[2]) * s)
            line = " ".join(f)
        elif f[0] == "LD":
            if int(f[2]) <= 0 or f[3] != f[4]:
                fail(f"LD over more than one segment or on tag 0: {line}")
            f[3] = f[4] = m(f[3])
            line = " ".join(f)
        elif f[0] == "EX" and f[1] != "1":
            fail(f"not a plane wave: {line}")
        elif f[0] == "EN" and not any(l.split()[:1] == ["XQ"] for l in text.split("\n")):
            out.append("XQ")  # nec2c solves nothing at EN without one
        out.append(line)
        if f[0] == "GE":
            out.append("EK 0")
    return "\n".join(out)


def currents(text):
    with tempfile.TemporaryDirectory(prefix="n", dir="/tmp") as t:
        i, o = Path(t) / "m.nec", Path(t) / "m.out"
        i.write_text(text)
        r = subprocess.run(["nec2c", "-i", str(i), "-o", str(o)], capture_output=True, text=True)
        if r.returncode or r.stderr.strip():
            fail(f"nec2c: {r.stderr.strip()[:300]}")
        rep = o.read_text().split("\n")
    k = next((i for i, l in enumerate(rep) if "CURRENTS AND LOCATION" in l), None)
    if k is None:
        fail("nec2c printed no current table")
    k += 5
    cur = []
    while ROW.match(rep[k]):
        f = rep[k].split()
        assert int(f[0]) == len(cur) + 1
        cur.append(complex(float(f[6]), float(f[7])))
        k += 1
    return cur


def main():
    if len(sys.argv) < 3:
        fail("usage: nec2c-receive-series.py DECK S [S ...]")
    text = Path(sys.argv[1]).read_text()
    ws = wires(text)
    res = {"kernel": "extended"}
    for s in (int(x) for x in sys.argv[2:]):
        for n, a, b, r in ws:
            length = sum((b[i] - a[i]) ** 2 for i in range(3)) ** 0.5
            if length / (n * s) / r <= 2.0:
                fail(f"{s}x: segment/radius {length / (n * s) / r:.2f} <= 2, below EK's limit")
        c = currents(remesh(text, s))
        n0 = len(c) // s
        cr = corner_ratio(ws, s)
        res[str(s)] = {
            "peak": max(abs(x) for x in c),
            "I": [[c[s * k - (s - 1) // 2 - 1].real, c[s * k - (s - 1) // 2 - 1].imag] for k in range(1, n0 + 1)],
            "corner_ratio": None if cr is None else round(cr, 2),
            "corner_below_10": cr is not None and cr < 10,
        }
    print(json.dumps(res))


main()

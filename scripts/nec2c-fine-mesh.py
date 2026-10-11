#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Write a deck meshed S times finer, every segment reference at the same point (FND-232).

    scripts/nec2c-fine-mesh.py DECK S > FINE.nec

For capturing a nec2c reference at its own converged value rather than at the
subject's mesh. Every GW's segment count is multiplied by S (odd), and every
segment reference k becomes S*k - (S-1)/2: the sub-segment whose midpoint is the
original segment's midpoint, so a gap, a lumped load or a network port stays
where it was.

Only the cards listed in KNOWN are accepted; anything else is refused, never
passed through, because a card this script does not map would silently refer to
a different segment. Refused too: a lumped load on a segment range (NEC applies it to
each segment, which one sub-segment each cannot reproduce); a tag-0 reference
with a segment number (an absolute index); and an S that takes a
wire below NEC's thin-wire limit (segment length / radius > 8 without EK, > 2
with it).
"""

import sys

KNOWN = {"CM", "CE", "GW", "GE", "GN", "EK", "EX", "LD", "NT", "TL", "PT", "FR", "RP", "NE", "NH", "XQ", "EN"}


def fail(msg):
    sys.exit(f"nec2c-fine-mesh: {msg}")


def main():
    if len(sys.argv) != 3:
        fail("usage: nec2c-fine-mesh.py DECK S")
    path, s = sys.argv[1], int(sys.argv[2])
    if s < 1 or s % 2 == 0:
        fail(f"S must be odd and positive, got {s}")
    lines = open(path).read().splitlines()
    cards = [(l.split()[0].upper() if l.split() else "", l) for l in lines]
    extended = any(c == "EK" and (l.split()[1:2] or ["0"])[0] != "-1" for c, l in cards)
    limit = 2.0 if extended else 8.0

    def seg(k):
        k = int(k)
        return str(s * k - (s - 1) // 2) if k > 0 else str(k)

    out = []
    for card, line in cards:
        f = line.split()
        if not f:
            out.append(line)
            continue
        if card not in KNOWN:
            fail(f"card {card!r} is not mapped: {line!r}")
        if card == "GW":
            tag, n = int(f[1]), int(f[2])
            x1, y1, z1, x2, y2, z2, r = map(float, f[3:10])
            length = ((x2 - x1) ** 2 + (y2 - y1) ** 2 + (z2 - z1) ** 2) ** 0.5
            ratio = length / (n * s) / r
            if ratio <= limit:
                fail(f"tag {tag} at {s}x: segment/radius {ratio:.2f} <= {limit:g} ({'EK' if extended else 'no EK'})")
            f[2] = str(n * s)
        elif card == "EX" and int(f[1]) in (0, 5):
            if int(f[2]) == 0:
                fail(f"a tag-0 source names an absolute segment, which this does not map: {line!r}")
            f[3] = seg(f[3])
        elif card == "EX":
            fail(f"EX type {f[1]} is not mapped: {line!r}")
        elif card == "LD":
            ld_type, k1, k2 = int(f[1]), int(f[3]), int(f[4])
            if k1 == 0 and k2 == 0:
                pass  # the whole tag: unchanged by a finer mesh
            elif int(f[2]) == 0:
                fail(f"a tag-0 load names absolute segments, which this does not map: {line!r}")
            elif ld_type in (2, 3, 5):
                # per-metre or conductivity: the whole of each original segment
                f[3], f[4] = str(s * (k1 - 1) + 1), str(s * k2)
            elif k1 == k2:
                f[3] = f[4] = seg(k1)
            else:
                fail(f"a lumped load over a segment range is not one sub-segment each: {line!r}")
        elif card in ("NT", "TL"):
            if int(f[1]) == 0 or int(f[3]) == 0:
                fail(f"a tag-0 port names an absolute segment, which this does not map: {line!r}")
            f[2], f[4] = seg(f[2]), seg(f[4])
        elif card == "PT":
            fail(f"PT is not mapped: {line!r}")
        out.append(" ".join(f))
    print("\n".join(out))


main()

#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# PH9-CHK-007 MPIE Phase B — the junction basis validated (concept oracle).
#
# The entire-domain Hallen junction prototype (docs/ph9-chk-002-general-junction.md)
# DIVERGED on the degree-3 Y-junction: its radiation resistance climbed to the
# wrong fixed point (~80 ohm) under mesh refinement. This script proves the fix:
# a mixed-potential EFIE (MPIE) with a leg-based triangle basis, where a degree-N
# junction node carries N-1 arm-pair "dipole" bases so Kirchhoff's current law is
# automatic (each dipole carries current IN on one arm and OUT on another, with no
# explicit KCL row). It CONVERGES monotonically to nec2c.
#
# Results (14.2 MHz, symmetric Y = 3 arms x 5 m at 120deg, feed at arm midpoint,
# a = 1 mm), even mesh, feed exactly at position 0.5:
#   arm= 10 :  68.75 - j696.2
#   arm= 20 :  69.33 - j354.6
#   arm= 40 :  69.84 - j192.8
#   arm= 80 :  70.41 - j116.9
#   arm=160 :  70.93 -  j83.4
# nec2c (live) at 11/21/41 seg/arm: 71.78/71.60/71.50 - j~67.
# R converges MONOTONICALLY toward 71.5 (Hallen instead diverged past 80). The
# reactance converges slowly (~1/N, the known delta-gap behavior) but monotonically
# from below; R is the quantity that matters and that Hallen got wrong.
#
# This is the concept oracle for the Rust port in crates/nec_solver/src/mpie.rs
# (build_bases / assemble / solve_mpie). The Rust reproduces these numbers.

import math
import cmath
import numpy as np

C0 = 299_792_458.0
MU0 = 4e-7 * math.pi
EPS0 = 8.8541878128e-12
F = 14.2e6
W = 2 * math.pi * F
K = W / C0
A = 0.001

GLN = [-0.932469514203152, -0.661209386466265, -0.238619186083197,
       0.238619186083197, 0.661209386466265, 0.932469514203152]
GLW = [0.171324492379170, 0.360761573048139, 0.467913934572691,
       0.467913934572691, 0.360761573048139, 0.171324492379170]


def gfree(dist):
    r = math.sqrt(dist * dist + A * A)
    return cmath.exp(-1j * K * r) / r


class Leg:
    """A segment leg of a triangle basis: shared node V at p1? current toward V?"""

    def __init__(s, seg, v_is_p1, toward):
        s.seg, s.v1, s.toward = seg, v_is_p1, toward


def seg_geo(nodes, segs):
    G = []
    for (n0, n1) in segs:
        p0, p1 = nodes[n0], nodes[n1]
        d = p1 - p0
        ln = np.linalg.norm(d)
        G.append((p0, p1, d / ln, ln))
    return G


def build_bases(nodes, segs):
    """Leg-based bases: degree-1 free ends none, degree-2 one, degree-N gives N-1
    arm-pair dipoles (KCL automatic). Returns (bases, node->deg2 feed basis)."""
    inc = {i: [] for i in range(len(nodes))}
    for si, (n0, n1) in enumerate(segs):
        inc[n0].append((si, False))
        inc[n1].append((si, True))
    bases, node_feed = [], {}
    for nd, legs_at in inc.items():
        deg = len(legs_at)
        if deg < 2:
            continue
        s0, v0 = legs_at[0]
        for k in range(1, deg):
            sk, vk = legs_at[k]
            if deg == 2:
                node_feed[nd] = len(bases)
            bases.append([Leg(s0, v0, True), Leg(sk, vk, False)])
    return bases, node_feed


def flow_tangent(leg, G):
    sign = (1.0 if leg.v1 else -1.0) * (1.0 if leg.toward else -1.0)
    return sign * G[leg.seg][2]


def charge(leg, G):
    return (1.0 if leg.toward else -1.0) / G[leg.seg][3]


def fscalar(leg, u):
    return u if leg.v1 else (1.0 - u)


def assemble(nodes, segs, bases):
    G = seg_geo(nodes, segs)
    nb = len(bases)
    Z = np.zeros((nb, nb), complex)
    preA = 1j * W * MU0 / (4 * math.pi)
    preP = 1.0 / (1j * W * EPS0 * 4 * math.pi)
    for m in range(nb):
        for n in range(m, nb):
            za = zp = 0j
            for lm in bases[m]:
                tm = flow_tangent(lm, G)
                cm = charge(lm, G)
                p0m, p1m, _, lnm = G[lm.seg]
                for ln_ in bases[n]:
                    tt = float(np.dot(tm, flow_tangent(ln_, G)))
                    cc = cm * charge(ln_, G)
                    p0n, p1n, _, lnn = G[ln_.seg]
                    for ga, wa0 in zip(GLN, GLW):
                        ua = 0.5 * (ga + 1)
                        ra = p0m + ua * (p1m - p0m)
                        wa = 0.5 * wa0 * lnm
                        fm = fscalar(lm, ua)
                        for gb, wb0 in zip(GLN, GLW):
                            ub = 0.5 * (gb + 1)
                            rb = p0n + ub * (p1n - p0n)
                            wb = 0.5 * wb0 * lnn
                            g = gfree(np.linalg.norm(ra - rb))
                            za += wa * wb * fm * fscalar(ln_, ub) * tt * g
                            zp += wa * wb * cc * g
            Z[m, n] = preA * za + preP * zp
            Z[n, m] = Z[m, n]
    return Z


def build_y(nseg_arm):
    L = 5.0
    dirs = [(1, 0, 0),
            (math.cos(2 * math.pi / 3), math.sin(2 * math.pi / 3), 0),
            (math.cos(4 * math.pi / 3), math.sin(4 * math.pi / 3), 0)]
    nodes = [(0.0, 0.0, 0.0)]
    segs = []
    for d in dirs:
        prev = 0
        for i in range(1, nseg_arm + 1):
            t = i / nseg_arm
            nodes.append((d[0] * L * t, d[1] * L * t, d[2] * L * t))
            idx = len(nodes) - 1
            segs.append((prev, idx))
            prev = idx
    return np.array(nodes), segs, nseg_arm // 2  # feed = arm-0 midpoint node


def solve_y(nseg_arm):
    nodes, segs, feed_node = build_y(nseg_arm)
    bases, node_feed = build_bases(nodes, segs)
    Z = assemble(nodes, segs, bases)
    feed = node_feed[feed_node]
    V = np.zeros(len(bases), complex)
    V[feed] = 1.0
    Zin = 1.0 / np.linalg.solve(Z, V)[feed]
    print(f"  arm={nseg_arm:3d} : {Zin.real:8.3f}{Zin.imag:+9.3f}j")
    return Zin


def main():
    print("Y-junction MPIE (nec2c 11/21/41 seg: 71.78/71.60/71.50 - j~67):")
    for ns in [10, 20, 40, 80]:
        solve_y(ns)


if __name__ == "__main__":
    main()

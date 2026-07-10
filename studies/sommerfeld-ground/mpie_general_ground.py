#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# PH9-CHK-007 MPIE Phase E — general-orientation Sommerfeld ground (validated concept).
#
# Phase D put the Sommerfeld ground into the MPIE Z-matrix for a HORIZONTAL wire via
# the reflected potential kernels (G_A, G_Phi). This script validates the ARBITRARY-
# orientation extension: the reflected term is computed as a Galerkin REACTION of the
# general reflected-E-field dyadic (general_dyadic.eproj_refl) added to the free-space
# potential-form Z. Two facts make this correct and drop-in:
#
#   1. The reflected mutual impedance equals -<f_m, E_reflected{f_n}>, which by
#      integration-by-parts equals the reflected potential form. So the E-field
#      reaction ADDS DIRECTLY to the free-space potential-form Z (no extra prefactor).
#      Confirmed here: the free-space E-field reaction reproduces the free-space
#      potential-form Z entry to ratio 1.0.
#   2. The general dyadic reduces to the horizontal kernels for a horizontal wire, so
#      the vertical/tilted case is the same physics.
#
# Results (14.2 MHz, epsr=13, sigma=0.005, N=40):
#   free-space off-diagonal:  potential Z == E-field reaction  (ratio 1.000)
#   vertical lambda/2 dipole, base 0.05lam, over GN2:
#       this concept:  ~84.8 + j35.8      nec2c: 89.75 + j38.52   (~6-7%)
#
# CAVEAT: this prototype uses the 2-D angular-spectrum dyadic (general_dyadic.eproj_refl),
# which is under-resolved in the low-d / large-rho pole corner (needs na >> 45*rho/d).
# That inflates the HORIZONTAL reactance here; the production Rust uses the exact 1-D
# reduction sommerfeld::reflected_e_projected_fast, which resolves the corner and
# reproduces the Phase-D horizontal result (a 2-deg-tilted wire -> 64.2 + j49.7,
# matching Phase D's 64.00 + j49.18). The VERTICAL case has rho=0 (no pole corner), so
# the number above is reliable.

import importlib.util
import numpy as np


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


mj = _load("mj", "mpie_junction.py")      # free-space potential-form MPIE (graph)
gd = _load("gd", "general_dyadic.py")     # eproj_refl / eproj_freespace
lam = gd.lam
GLN, GLW = mj.GLN, mj.GLW


def axis_dipole(nseg, axis, base_h):
    """Straight lambda/2 dipole along `axis`, lowest point at height base_h."""
    axis = np.array(axis, float)
    axis /= np.linalg.norm(axis)
    half = lam / 4
    nodes = np.array([axis * (-half + i / nseg * 2 * half) for i in range(nseg + 1)])
    nodes[:, 2] += base_h - nodes[:, 2].min()
    segs = [(i, i + 1) for i in range(nseg)]
    return nodes, segs, axis


def reaction_entry(nodes, segs, bases, m, n, kernel):
    """A single reaction entry Z[m][n] = sum_legs int int f_m f_n kernel(...)."""
    G = mj.seg_geo(nodes, segs)
    acc = 0j
    for lm in bases[m]:
        tm = mj.flow_tangent(lm, G)
        p0m, p1m, _, lnm = G[lm.seg]
        for ln_ in bases[n]:
            tn = mj.flow_tangent(ln_, G)
            p0n, p1n, _, lnn = G[ln_.seg]
            for ga, wa0 in zip(GLN, GLW):
                ua = 0.5 * (ga + 1)
                ra = p0m + ua * (p1m - p0m)
                wa, fm = 0.5 * wa0 * lnm, mj.fscalar(lm, ua)
                for gb, wb0 in zip(GLN, GLW):
                    ub = 0.5 * (gb + 1)
                    rb = p0n + ub * (p1n - p0n)
                    wb, fn = 0.5 * wb0 * lnn, mj.fscalar(ln_, ub)
                    acc += wa * wb * fm * fn * kernel(tn, tm, ra, rb)
    return acc


def solve_vertical_gn2(nseg=40, ng=44):
    """Vertical dipole: reflected reaction on a (ds, sg) grid + interpolation."""
    nodes, segs, axis = axis_dipole(nseg, [0, 0, 1], 0.05 * lam)
    bases, node_feed = mj.build_bases(nodes, segs)
    Z = mj.assemble(nodes, segs, bases).astype(complex)
    G = mj.seg_geo(nodes, segs)
    c = nodes.mean(axis=0)
    s = (nodes - c) @ axis
    smin, smax = s.min(), s.max()
    ds_lo, ds_sp, sg_lo, sg_sp = smin - smax, 2 * (smax - smin), 2 * smin, 2 * (smax - smin)
    grid = np.array([[gd.eproj_refl(list(axis), list(axis),
                                    (ds_lo + ds_sp * i / (ng - 1)) * axis[0],
                                    (ds_lo + ds_sp * i / (ng - 1)) * axis[1],
                                    2 * c[2] + (sg_lo + sg_sp * k / (ng - 1)) * axis[2],
                                    pec=False, na=96, nr=800)
                      for k in range(ng)] for i in range(ng)])

    def interp(ds, sg):
        fi = np.clip((ds - ds_lo) / ds_sp * (ng - 1), 0, ng - 1)
        fk = np.clip((sg - sg_lo) / sg_sp * (ng - 1), 0, ng - 1)
        i0, k0 = min(int(fi), ng - 2), min(int(fk), ng - 2)
        a, b = fi - i0, fk - k0
        return (grid[i0, k0] * (1 - a) * (1 - b) + grid[i0 + 1, k0] * a * (1 - b)
                + grid[i0, k0 + 1] * (1 - a) * b + grid[i0 + 1, k0 + 1] * a * b)

    nb = len(bases)
    for m in range(nb):
        for n in range(m, nb):
            acc = 0j
            for lm in bases[m]:
                sig_m = np.dot(mj.flow_tangent(lm, G), axis)
                p0m, p1m, _, lnm = G[lm.seg]
                for ln_ in bases[n]:
                    sig = sig_m * np.dot(mj.flow_tangent(ln_, G), axis)
                    p0n, p1n, _, lnn = G[ln_.seg]
                    for ga, wa0 in zip(GLN, GLW):
                        ua = 0.5 * (ga + 1)
                        sa = np.dot(p0m + ua * (p1m - p0m) - c, axis)
                        wa, fm = 0.5 * wa0 * lnm, mj.fscalar(lm, ua)
                        for gb, wb0 in zip(GLN, GLW):
                            ub = 0.5 * (gb + 1)
                            sb = np.dot(p0n + ub * (p1n - p0n) - c, axis)
                            wb, fn = 0.5 * wb0 * lnn, mj.fscalar(ln_, ub)
                            acc += wa * wb * fm * fn * sig * interp(sa - sb, sa + sb)
            Z[m, n] += acc
            Z[n, m] += acc
    feed = node_feed[nseg // 2]
    V = np.zeros(nb, complex)
    V[feed] = 1.0
    return 1.0 / np.linalg.solve(Z, V)[feed]


def check_reaction_consistency(nseg=20):
    """A far-apart (non-singular) off-diagonal: potential-form == E-field reaction."""
    nodes, segs, _ = axis_dipole(nseg, [1, 0, 0], 0.0)
    bases, _ = mj.build_bases(nodes, segs)
    Zpot = mj.assemble(nodes, segs, bases)
    m, n = 3, 15
    kern = lambda tn, tm, ra, rb: gd.eproj_freespace(
        list(tn), list(tm), ra[0] - rb[0], ra[1] - rb[1], ra[2] - rb[2])
    return Zpot[m, n], reaction_entry(nodes, segs, bases, m, n, kern)


def main():
    zp, zr = check_reaction_consistency()
    print("Free-space E-field reaction reproduces the potential-form Z entry:")
    print(f"  potential {zp:.5e}")
    print(f"  reaction  {zr:.5e}   ratio {zp / zr:.5f}")
    print("\nVertical lambda/2 dipole, base 0.05lam, over GN2 (nec2c 89.75+j38.52):")
    print(f"  {solve_vertical_gn2()}")


if __name__ == "__main__":
    main()

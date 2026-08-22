#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# PH9-CHK-006 Level-2 DCIM — PHASE 2 end-to-end validation (BL-IMPR-015).
#
# Phase 1 (dcim_probe.py) validated the DCIM complex-image *fit* against the exact
# reflected potential kernels pointwise, and found G_A degrades in the far zone
# (ρ ≳ 0.5λ) — the lateral-wave/branch-cut tail that spherical complex images
# cannot represent. But the MoM Z-matrix only needs the kernel for ρ ≤ the wire
# half-length (~0.52λ here), and the far-field error sits on small distant matrix
# entries. So the decisive test is END-TO-END: does the DCIM kernel, dropped into
# the validated EFIE-MoM (efie_mpie_ground.py), reproduce the feedpoint Z that the
# EXACT kernel — and nec2c GN2 — give?
#
# This reuses efie_mpie_ground's Galerkin triangle-basis MoM unchanged and swaps
# only the reflected kernel: exact Sommerfeld quadrature vs DCIM complex images.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import numpy as np

import efie_mpie_ground as em
from dcim_probe import dcim_fit, dcim_fit_2level, dcim_kernel, f_phi, r_te

# Fit the two spectral coefficients once (geometry-independent complex images).
# Two-level DCIM for both kernels (near + far image sets).
A_TE, B_TE = dcim_fit_2level(r_te)
A_PH, B_PH = dcim_fit_2level(f_phi)

# Cache the free-space Z-matrix (independent of ground / kernel).
ZFREE = em.zmat_free()
PRE_A = 1j * em.W * em.MU0 / (4 * np.pi)
PRE_P = 1.0 / (1j * em.W * em.EPS0 * 4 * np.pi)


def ground_z(height, ga_fn, gp_fn, n_grid=240):
    """Feedpoint Z of the horizontal λ/2 dipole at `height`, using ground kernels
    ga_fn(ρ,d)=G_A and gp_fn(ρ,d)=G_Φ. Mirrors efie_mpie_ground.solve_ground."""
    d = 2.0 * height
    rg = np.linspace(0.0, em.L * 1.05, n_grid)
    GA = np.array([ga_fn(max(r, 1e-6), d) for r in rg])
    GP = np.array([gp_fn(max(r, 1e-6), d) for r in rg])
    ia = lambda r: np.interp(r, rg, GA.real) + 1j * np.interp(r, rg, GA.imag)
    ip = lambda r: np.interp(r, rg, GP.real) + 1j * np.interp(r, rg, GP.imag)

    Z = ZFREE.copy()
    for m in range(em.NB):
        for n in range(em.NB):
            za = zp = 0j
            for (ma, mfp) in em.seglist(m):
                xm = [em.XN[ma] + (g + 1) / 2 * em.DL for g in em.GLN]
                wm = [w / 2 * em.DL for w in em.GLW]
                for (na, nfp) in em.seglist(n):
                    xn = [em.XN[na] + (g + 1) / 2 * em.DL for g in em.GLN]
                    wn = [w / 2 * em.DL for w in em.GLW]
                    for xa, wa in zip(xm, wm):
                        fm = em.tri_val(m, ma, xa)
                        for xb, wb in zip(xn, wn):
                            fn = em.tri_val(n, na, xb)
                            rho = abs(xa - xb)
                            za += wa * wb * fm * fn * ia(rho)
                            zp += wa * wb * mfp * nfp * ip(rho)
            Z[m, n] += PRE_A * za + PRE_P * zp
    V = np.zeros(em.NB, complex)
    V[em.FEED] = 1.0
    return 1.0 / np.linalg.solve(Z, V)[em.FEED]


def main():
    ga_exact = lambda r, d: em.sommerfeld("A", r, d, False)
    gp_exact = lambda r, d: em.sommerfeld("P", r, d, False)
    ga_dcim = lambda r, d: dcim_kernel(r, d, A_TE, B_TE)
    gp_dcim = lambda r, d: dcim_kernel(r, d, A_PH, B_PH)

    print("DCIM Phase-2 end-to-end: feedpoint Z of a horizontal λ/2 dipole over")
    print("GN2 (εr=13, σ=0.005, 14.2 MHz). DCIM images: "
          f"G_A={len(A_TE)}  G_Φ={len(A_PH)}\n")
    print(f"{'height':>10} {'exact-kernel MoM':>20} {'DCIM MoM':>20} {'nec2c GN2':>16}")
    for hl, ref in ((0.05, "67.26+j52.61"), (0.025, "87.81+j68.64")):
        h = hl * em.LAM
        ze = ground_z(h, ga_exact, gp_exact)
        zd = ground_z(h, ga_dcim, gp_dcim)
        drel = abs(zd - ze) / abs(ze)
        print(f"{hl:>9}λ  {ze.real:8.2f}{ze.imag:+8.2f}j   {zd.real:8.2f}{zd.imag:+8.2f}j"
              f"   {ref:>14}   |Δ(DCIM,exact)|={drel:.1%}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# PH9-CHK-006 Level 2 — the FULL EFIE reproduces nec2c GN2 (validated concept).
#
# The Level-2 architecture probe (level2_architecture_probe.py) ruled out patching
# fnec's Hallen solve perturbatively. This script proves the rigorous route works: a
# mixed-potential EFIE (MPIE) with a triangle (piecewise-linear) basis, Galerkin
# tested, with the Sommerfeld reflected VECTOR-potential and SCALAR-potential kernels
# in the impedance matrix — reproduces nec2c GN2 to ~5% (R and X), including the
# absolute reactance (no Hallen offset), the PEC image cancellation, and the surface
# wave. The current comes out correct too (not just the feedpoint Z).
#
# Results (14.2 MHz, horizontal lambda/2 dipole, eps_r=13, sigma=0.005, N=40):
#   free space : 74.36 + j41.36   (nec2c 78.85 + j44.70)   -- ~6% (discretization)
#   PEC 0.05L  :  5.87 + j34.11   (nec2c GN1 6.16 + j38.18) -- image cancellation OK
#   GN2 0.05L  : 64.00 + j49.18   (nec2c GN2 67.26 + j52.61)
#   GN2 0.025L : 83.46 + j66.26   (nec2c GN2 87.81 + j68.64)
#
# Key formulation points:
#   * MPIE keeps the scalar potential explicit (unlike Hallen, which eliminates it) —
#     the surface wave lives in the scalar-potential kernel, so this captures it.
#   * Reflected kernels: G_A = -j*S{R_TE},  G_Phi = -j*S{(k0^2 R_TE + kz0^2 R_TM)/lambda^2},
#     with S{f}(rho,d) = integral_0^inf (lambda/kz0) f J0(lambda*rho) e^{-j kz0 d} dlambda.
#     The -j is essential: the Sommerfeld identity gives S{1} = +j * e^{-jk r_img}/r_img,
#     so the reflected Green's functions need the -j to match the direct G = e^{-jkR}/R.
#   * Reduced kernel R = sqrt(dx^2 + a^2) regularizes the direct self/near terms (thin
#     wire); the sin/cosh radial substitution regularizes the Sommerfeld integrals.
#
# This validates the concept. A production Rust Level 2 is a large increment (a new
# MPIE solver path + reflected potential kernels + the full dyadic for arbitrary
# orientation via the 3-scalar set V_TE=S{R_TE}, V_TM=S{R_TM}, U=S{(R_TE+R_TM)/l^2}).

import math
import cmath
import numpy as np
from scipy.special import jv

C0 = 299_792_458.0
MU0 = 4e-7 * math.pi
EPS0 = 8.8541878128e-12
F = 14.2e6
W = 2 * math.pi * F
K = W / C0
LAM = C0 / F
L = LAM / 2
A = 0.001
N = 40
DL = L / N
XN = np.array([-L / 2 + k * DL for k in range(N + 1)])
NB = N - 1
FEED = NB // 2
EPSR, SIGMA = 13.0, 0.005
EPSC = EPSR - 1j * SIGMA / (W * EPS0)
KG2 = K * K * EPSC

GLN = [-0.932469514203152, -0.661209386466265, -0.238619186083197,
       0.238619186083197, 0.661209386466265, 0.932469514203152]
GLW = [0.171324492379170, 0.360761573048139, 0.467913934572691,
       0.467913934572691, 0.360761573048139, 0.171324492379170]


def gfree(dx):
    r = math.sqrt(dx * dx + A * A)
    return cmath.exp(-1j * K * r) / r


def seglist(n):
    return [(n, +1.0 / DL), (n + 1, -1.0 / DL)]


def tri_val(n, seg_a, x):
    return (x - XN[n]) / DL if seg_a == n else (XN[n + 2] - x) / DL


def zmat_free():
    Z = np.zeros((NB, NB), complex)
    preA = 1j * W * MU0 / (4 * math.pi)
    preP = 1.0 / (1j * W * EPS0 * 4 * math.pi)
    for m in range(NB):
        for n in range(NB):
            za = zp = 0j
            for (ma, mfp) in seglist(m):
                xm = [XN[ma] + (g + 1) / 2 * DL for g in GLN]
                wm = [w / 2 * DL for w in GLW]
                for (na, nfp) in seglist(n):
                    xn = [XN[na] + (g + 1) / 2 * DL for g in GLN]
                    wn = [w / 2 * DL for w in GLW]
                    for xa, wa in zip(xm, wm):
                        fm = tri_val(m, ma, xa)
                        for xb, wb in zip(xn, wn):
                            fn = tri_val(n, na, xb)
                            g = gfree(xa - xb)
                            za += wa * wb * fm * fn * g
                            zp += wa * wb * mfp * nfp * g
            Z[m, n] = preA * za + preP * zp
    return Z


def kz1(l):
    s = cmath.sqrt(KG2 - l * l)
    return -s if s.imag > 0 else s


def sommerfeld(kind, rho, d, pec=False, nr=3000):
    """Reflected Green's function: -j S{f}. kind 'A'=R_TE, 'P'=(k0^2 R_TE+kz0^2 R_TM)/l^2."""
    tot = 0j
    th = np.linspace(1e-7, math.pi / 2 - 1e-7, nr)
    lp, ap = K * np.sin(th), K * np.cos(th)
    tt = np.linspace(1e-7, math.asinh(45 / (K * d)), nr)
    le, ae = K * np.cosh(tt), -1j * K * np.sinh(tt)
    for (l, a, dv, wr) in ((lp, ap, th, K * np.sin(th)), (le, ae, tt, 1j * K * np.cosh(tt))):
        if pec:
            rte, rtm = -1.0 + 0 * a, 1.0 + 0 * a
        else:
            b = np.array([kz1(x) for x in l])
            rte, rtm = (a - b) / (a + b), (EPSC * a - b) / (EPSC * a + b)
        f = rte if kind == 'A' else (K * K * rte + a * a * rtm) / (l * l)
        tot += np.trapezoid(wr * f * jv(0, l * rho) * np.exp(-1j * a * d), dv)
    return -1j * tot


def solve_ground(Zfree, V, H, pec, ref, label):
    d = 2 * H
    rg = np.linspace(0.0, L * 1.05, 240)
    GA = np.array([sommerfeld('A', max(r, 1e-6), d, pec) for r in rg])
    GP = np.array([sommerfeld('P', max(r, 1e-6), d, pec) for r in rg])
    ia = lambda r: np.interp(r, rg, GA.real) + 1j * np.interp(r, rg, GA.imag)
    ip = lambda r: np.interp(r, rg, GP.real) + 1j * np.interp(r, rg, GP.imag)
    Z = Zfree.copy()
    preA = 1j * W * MU0 / (4 * math.pi)
    preP = 1.0 / (1j * W * EPS0 * 4 * math.pi)
    for m in range(NB):
        for n in range(NB):
            za = zp = 0j
            for (ma, mfp) in seglist(m):
                xm = [XN[ma] + (g + 1) / 2 * DL for g in GLN]
                wm = [w / 2 * DL for w in GLW]
                for (na, nfp) in seglist(n):
                    xn = [XN[na] + (g + 1) / 2 * DL for g in GLN]
                    wn = [w / 2 * DL for w in GLW]
                    for xa, wa in zip(xm, wm):
                        fm = tri_val(m, ma, xa)
                        for xb, wb in zip(xn, wn):
                            fn = tri_val(n, na, xb)
                            rho = abs(xa - xb)
                            za += wa * wb * fm * fn * ia(rho)
                            zp += wa * wb * mfp * nfp * ip(rho)
            Z[m, n] += preA * za + preP * zp
    I = np.linalg.solve(Z, V)
    Z_in = 1.0 / I[FEED]
    print(f"{label}: {Z_in.real:.2f}{Z_in.imag:+.2f}j  (nec2c {ref})")


def main():
    Zf = zmat_free()
    V = np.zeros(NB, complex)
    V[FEED] = 1.0
    Zin = 1.0 / np.linalg.solve(Zf, V)[FEED]
    print(f"free space : {Zin.real:.2f}{Zin.imag:+.2f}j  (nec2c 78.85+j44.70)")
    solve_ground(Zf, V, 0.05 * LAM, True, "GN1 6.16+j38.18", "PEC  0.05λ ")
    solve_ground(Zf, V, 0.05 * LAM, False, "GN2 67.26+j52.61", "GN2  0.05λ ")
    solve_ground(Zf, V, 0.025 * LAM, False, "GN2 87.81+j68.64", "GN2  0.025λ")


if __name__ == "__main__":
    main()

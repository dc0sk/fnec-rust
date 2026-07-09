#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# PH9-CHK-006 Levels 1 & 2 foundation: the GENERAL reflected half-space dyadic.
#
# The shipped Level-0 kernel (horizontal_dipole_sommerfeld.py) is the φ=0,
# x-source/x-obs slice. This script validates the full reflected E-field dyadic for
# ARBITRARY source/observation orientations and arbitrary horizontal offset — the
# shared foundation of Level 1 (arbitrary-orientation feedpoint ΔZ correction) and
# Level 2 (Sommerfeld kernel in the Z-matrix via DCIM). See the "Generalization
# roadmap" section of docs/ph9-chk-006-sommerfeld-ground.md.
#
# Reflected E along observation direction d̂_o from a unit current moment along source
# direction d̂_s, horizontal offset (ΔX,ΔY), height-sum d = z_o + h_s (plane-wave /
# angular spectrum):
#
#   E_proj = (k0·η0 / 8π²) ∬ (1/kz0) e^{-j kz0 d} e^{-j(kx ΔX + ky ΔY)}
#              [ (d̂_s·ŝ)(d̂_o·ŝ) R_TE + (d̂_s·p̂_i)(d̂_o·p̂_r) R_TM ] dkx dky
#
# evaluated as a 2-D integral (radial sinθ/cosh t substitution × azimuth grid).
#
# Results (14.2 MHz, εr=13, σ=0.005):
#   * PEC self-check: the 2-D dyadic with (R_TE,R_TM)=(-1,+1) reproduces the exact
#     free-space image-dyadic field (image dir (-sx,-sy,+sz)) to ~1e-6 for EVERY
#     orientation pair (x/x on- & off-axis, vertical z/z, cross x/z, bent x/y,
#     tilted 45°).
#   * End-to-end: a 30°-tilted λ/2 dipole low over ground — the general-dyadic
#     reaction ΔZ correction gives ΔR +9.6 (nec2c GN2−GN0 gap +10.4); applied to
#     fnec's RCM ΔR −2.2 → +7.4 vs nec2c GN2 +8.1 (<10%).

import numpy as np

c = 299_792_458.0
f = 14.2e6
eps0 = 8.8541878128e-12
mu0 = 4e-7 * np.pi
w = 2 * np.pi * f
k0 = w / c
lam = c / f
eta0 = np.sqrt(mu0 / eps0)
epsr, sigma = 13.0, 0.005
epsc = epsr - 1j * sigma / (w * eps0)
kg2 = k0 * k0 * epsc


def kz1_of(l):
    s = np.sqrt(kg2 - l * l + 0j)
    return np.where(s.imag > 0, -s, s)


def r_te(l, a, pec):
    if pec:
        return -1.0 + 0 * a
    b = kz1_of(l)
    return (a - b) / (a + b)


def r_tm(l, a, pec):
    if pec:
        return 1.0 + 0 * a
    b = kz1_of(l)
    return (epsc * a - b) / (epsc * a + b)


def eproj_refl(ds, do, dX, dY, d, pec=False, na=192, nr=1500):
    """Reflected E along do from a unit current moment along ds (2-D spectral integral)."""
    sx, sy, sz = ds
    ox, oy, oz = do
    alphas = np.linspace(0, 2 * np.pi, na, endpoint=False)
    dal = alphas[1] - alphas[0]
    th = np.linspace(1e-9, np.pi / 2 - 1e-9, nr)
    lp, ap, wp = k0 * np.sin(th), k0 * np.cos(th), k0 * np.sin(th)
    tmax = np.arcsinh(45.0 / (k0 * d)) if d > 0 else 8.0
    tt = np.linspace(1e-9, tmax, nr)
    le, ae, we = k0 * np.cosh(tt), -1j * k0 * np.sinh(tt), 1j * k0 * np.cosh(tt)
    tot = 0j
    for al in alphas:
        ca, sa = np.cos(al), np.sin(al)
        for (l, a, wr, dv) in ((lp, ap, wp, th), (le, ae, we, tt)):
            s_ds = -sx * sa + sy * ca
            s_do = -ox * sa + oy * ca
            pi_ds = -(a * (sx * ca + sy * sa) + sz * l) / k0
            pr_do = (a * (ox * ca + oy * sa) - oz * l) / k0
            proj = s_ds * s_do * r_te(l, a, pec) + pi_ds * pr_do * r_tm(l, a, pec)
            phase = np.exp(-1j * a * d) * np.exp(-1j * l * (dX * ca + dY * sa))
            tot += np.trapezoid(wr * proj * phase, dv) * dal
    return (k0 * eta0 / (8 * np.pi * np.pi)) * tot


def eproj_freespace(ds, do, dX, dY, dZ):
    R = np.array([dX, dY, dZ])
    r = np.linalg.norm(R)
    g = np.exp(-1j * k0 * r) / (4 * np.pi * r)
    gp = -(1 + 1j * k0 * r) * np.exp(-1j * k0 * r) / (4 * np.pi * r * r)
    gpp = np.exp(-1j * k0 * r) * (2 + 2j * k0 * r - k0 * k0 * r * r) / (4 * np.pi * r ** 3)
    ds, do = np.array(ds, float), np.array(do, float)
    d2 = np.empty((3, 3), complex)
    for i in range(3):
        for j in range(3):
            dij = 1.0 if i == j else 0.0
            d2[i, j] = (R[i] * R[j] / (r * r)) * gpp + (dij / r - R[i] * R[j] / r ** 3) * gp
    return 1j * w * mu0 * ((do @ ds) * g + (do @ d2 @ ds) / (k0 * k0))


def eproj_image_pec(ds, do, dX, dY, d):
    return eproj_freespace([-ds[0], -ds[1], ds[2]], do, dX, dY, d)


def gamma_scalar():
    s = np.sqrt(epsc)
    return (s - 1) / (s + 1)


def main():
    print("=== PEC self-check: 2-D reflected dyadic vs free-space image dyadic ===")
    cases = [
        ("x-src x-obs on-axis", [1, 0, 0], [1, 0, 0], 0.3 * lam, 0.0),
        ("x-src x-obs off-axis", [1, 0, 0], [1, 0, 0], 0.2 * lam, 0.25 * lam),
        ("z-src z-obs vertical", [0, 0, 1], [0, 0, 1], 0.2 * lam, 0.0),
        ("x-src z-obs cross", [1, 0, 0], [0, 0, 1], 0.25 * lam, 0.1 * lam),
        ("bent x-src y-obs", [1, 0, 0], [0, 1, 0], 0.15 * lam, 0.2 * lam),
        ("tilted 45 src/obs", list(np.array([1, 0, 1]) / 2 ** 0.5),
         list(np.array([0, 1, 1]) / 2 ** 0.5), 0.2 * lam, 0.15 * lam),
    ]
    for name, ds, do, dX, dY in cases:
        for hl in (0.1, 0.03):
            d = 2 * hl * lam
            rel = abs(eproj_refl(ds, do, dX, dY, d, pec=True) - eproj_image_pec(ds, do, dX, dY, d)) \
                / abs(eproj_image_pec(ds, do, dX, dY, d))
            print(f" {name:22s} h={hl}λ: rel={rel:.2e}")

    print("\n=== End-to-end: 30°-tilted λ/2 dipole reaction ΔZ correction vs nec2c ===")
    d_hat = np.array([0.8660254, 0, 0.5])
    L, ctr, n = 5.278, np.array([0, 0, 3.0]), 21
    s = np.linspace(-L, L, n)
    mids = np.array([ctr + si * d_hat for si in s])
    lens = np.full(n, 2 * L / (n - 1))
    cur = np.sin(k0 * (L - np.abs(s)))
    g = gamma_scalar()
    acc = 0j
    for m in range(n):
        for kk in range(n):
            dX, dY = mids[m, 0] - mids[kk, 0], mids[m, 1] - mids[kk, 1]
            d = mids[m, 2] + mids[kk, 2]
            es = eproj_refl(list(d_hat), list(d_hat), dX, dY, d, pec=False, na=96, nr=800)
            ep = eproj_refl(list(d_hat), list(d_hat), dX, dY, d, pec=True, na=96, nr=800)
            acc += (es - g * ep) * (cur[m] * lens[m]) * (cur[kk] * lens[kk])
    print(f" my correction ΔR={acc.real:+.2f} ΔX={acc.imag:+.2f} | nec2c GN2−GN0 ΔR=+10.4")
    print(f" fnec RCM ΔR −2.2 + {acc.real:+.1f} = {-2.2 + acc.real:.1f} vs nec2c GN2 +8.1")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# PH9-CHK-006 Sommerfeld ground feasibility probe.
#
# Goal: prove that a direct Sommerfeld-integral reflected kernel for a HORIZONTAL
# dipole over a lossy half-space reproduces nec2c's exact GN2 near-ground impedance
# ΔZ — in particular the surface-wave SIGN FLIP below 0.1 λ that fnec's scalar-Γ
# (RCM / GN0) model gets wrong — before committing to a Rust implementation.
#
# Result (14.2 MHz, εr=13, σ=0.005, horizontal λ/2 dipole, ΔZ vs free space):
#   * PEC self-check: the reflected-field integral (R_TE=-1, R_TM=+1) reproduces the
#     exact opposite-current image field to a few % (field level) — validates the
#     kernel prefactors/signs and the TE/TM split. (The residual is uniform-grid
#     sampling of the integrable singularity at λ=k0, where 1/kz0 diverges; a
#     production impl deforms the contour past k0 or uses DCIM complex images.)
#   * ΔZ pipeline (induced-EMF reaction integral, assumed sinusoidal current):
#     reproduces nec2c PEC (GN1) ΔZ to ~7-8%.
#   * GN2: reproduces nec2c GN2 ΔR across 0.25→0.025 λ, INCLUDING the sign flip at
#     0.025 λ (mine +10.8 vs GN2 +9.0, where RCM/GN0 gives a wrong-signed -24.3).
#   The residual (~20% at the lowest height) is the assumed-sinusoidal-current
#   error; fnec's actual solved current would tighten it (reaction ΔZ is stationary
#   in the current to first order).
#
# Formulation (derived via the plane-wave / angular spectrum, azimuth reduced to a
# 1-D Sommerfeld integral over the radial spectral variable λ; cross-checked against
# an independent Michalski-Zheng mixed-potential derivation):
#
#   E_x^refl(ρ,d) = (ωμ0/8π) ∫_0^∞ (λ/kz0) e^{-j kz0 d}
#                     [ R_TE (J0(λρ)+J2(λρ)) - R_TM (kz0²/k0²)(J0(λρ)-J2(λρ)) ] dλ
#
# with d = z + z' (sum of source & obs heights), kz0 = √(k0²-λ²) (Im ≤ 0),
# R_TE = (kz0-kz1)/(kz0+kz1), R_TM = (εc kz0 - kz1)/(εc kz0 + kz1), kz1 = √(kg²-λ²),
# kg = k0√εc, εc = εr - jσ/(ωε0). The whole gap is the HORIZONTAL dipole; the
# vertical case (pure TM) is already accurate with fnec's scalar Γ.
#
# nec2c references regenerated with: GW horiz dipole + GN {0|1|2} 0 0 0 13 0.005.

import numpy as np
from scipy.special import jv

c = 299_792_458.0
f = 14.2e6
eps0 = 8.8541878128e-12
mu0 = 4e-7 * np.pi
w = 2 * np.pi * f
k0 = w / c
lam = c / f

epsr, sigma = 13.0, 0.005
epsc = epsr - 1j * sigma / (w * eps0)
kg = k0 * np.sqrt(epsc)


def kz(kk, l):
    """√(kk² - l²) on the sheet Im ≤ 0 (upgoing wave decays)."""
    s = np.sqrt(kk * kk - l * l + 0j)
    return np.where(s.imag > 0, -s, s)


def R_TE(l, pec):
    if pec:
        return -1.0 + 0 * l
    a, b = kz(k0, l), kz(kg, l)
    return (a - b) / (a + b)


def R_TM(l, pec):
    if pec:
        return 1.0 + 0 * l
    a, b = kz(k0, l), kz(kg, l)
    return (epsc * a - b) / (epsc * a + b)


def Ex_refl(rho, d, pec=False, lmax_fac=60.0, N=60000):
    """Reflected E_x at horizontal offset rho, height-sum d, per unit x current moment."""
    l = np.linspace(1e-6, k0 * lmax_fac, N)
    a = kz(k0, l)
    integ = (l / a) * np.exp(-1j * a * d) * (
        R_TE(l, pec) * (jv(0, l * rho) + jv(2, l * rho))
        - R_TM(l, pec) * (a * a / (k0 * k0)) * (jv(0, l * rho) - jv(2, l * rho))
    )
    return (w * mu0 / (8 * np.pi)) * np.trapezoid(integ, l)


def Ex_freespace_element(dx, dy, dz):
    """Exact E_x of an x-directed Hertzian element (moment I·l = 1) in free space."""
    r = np.sqrt(dx * dx + dy * dy + dz * dz)
    g = np.exp(-1j * k0 * r) / (4 * np.pi * r)
    gp = -(1 + 1j * k0 * r) * np.exp(-1j * k0 * r) / (4 * np.pi * r * r)
    gpp = np.exp(-1j * k0 * r) * (2 + 2j * k0 * r - k0 * k0 * r * r) / (4 * np.pi * r ** 3)
    d2g_dx2 = (dx * dx / (r * r)) * gpp + (1.0 / r - dx * dx / r ** 3) * gp
    return 1j * w * mu0 * (g + d2g_dx2 / (k0 * k0))


def Ex_image_pec(rho, h):
    """PEC image: reversed-current x-element at (0,0,-h), obs at (rho,0,h)."""
    return -1.0 * Ex_freespace_element(rho, 0.0, 2 * h)


# ---- induced-EMF reaction integral (assumed sinusoidal current) ----
L = lam / 4.0
NW = 81
xs = np.linspace(-L, L, NW)
Iw = np.sin(k0 * (L - np.abs(xs)))
dxw = xs[1] - xs[0]
RG = np.linspace(0.0, 2 * L, 160)


def deltaZ(Efun):
    Eg = np.array([Efun(r) for r in RG])
    acc = 0j
    for i in range(NW):
        e = np.interp(np.abs(xs[i] - xs), RG, Eg.real) + 1j * np.interp(
            np.abs(xs[i] - xs), RG, Eg.imag
        )
        acc += Iw[i] * np.sum(e * Iw) * dxw * dxw
    return acc  # I0 = I(0) = sin(k0 L) = 1


def main():
    print("=== PEC field self-check: Ex_refl(R_TE=-1,R_TM=+1) vs opposite-current image ===")
    for hl in (0.25, 0.1, 0.05):
        h = hl * lam
        for rl in (0.05, 0.15, 0.3):
            rho = rl * lam
            es = Ex_refl(rho, 2 * h, pec=True, lmax_fac=80.0, N=300000)
            ei = Ex_image_pec(rho, h)
            print(f" h={hl}λ ρ={rl}λ: rel={abs(es - ei) / abs(ei):.2e}")

    FS = 78.85 + 44.70j
    refPEC = {0.25: 95.086 + 76.177j, 0.10: 23.855 + 67.672j,
              0.05: 6.1596 + 38.179j, 0.025: 1.5276 + 19.658j}
    refGN2 = {0.25: 89.855 + 60.325j, 0.10: 59.638 + 58.137j,
              0.05: 67.264 + 52.611j, 0.025: 87.810 + 68.640j}
    refGN0 = {0.25: 90.444 + 61.548j, 0.10: 51.816 + 62.808j,
              0.05: 46.466 + 63.585j, 0.025: 54.584 + 146.93j}

    print("\n=== PEC ΔZ pipeline (closed-form image) vs nec2c GN1 ===")
    for hl in (0.25, 0.10, 0.05, 0.025):
        h = hl * lam
        dz = deltaZ(lambda r, h=h: Ex_image_pec(r, h))
        rr = refPEC[hl] - FS
        print(f" h={hl}λ: mine ΔZ={dz.real:+7.2f}{dz.imag:+7.2f}j | nec2c {rr.real:+7.2f}{rr.imag:+7.2f}j")

    print("\n=== GN2 (lossy Sommerfeld) vs nec2c GN2 [RCM/GN0 for contrast] ===")
    for hl in (0.25, 0.10, 0.05, 0.025):
        h = hl * lam
        lf, N = (20.0, 30000) if hl >= 0.1 else (60.0, 60000)
        dz = deltaZ(lambda r, h=h, lf=lf, N=N: Ex_refl(r, 2 * h, pec=False, lmax_fac=lf, N=N))
        r2, r0 = refGN2[hl] - FS, refGN0[hl] - FS
        print(f" h={hl}λ: mine ΔR={dz.real:+6.1f} | GN2 {r2.real:+6.1f} | GN0 {r0.real:+6.1f}")


if __name__ == "__main__":
    main()

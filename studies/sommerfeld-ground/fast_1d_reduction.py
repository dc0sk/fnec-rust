#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# PH9-CHK-006 Level 1: the 1-D azimuthal reduction of the general reflected dyadic.
#
# The 2-D angular-spectrum dyadic (general_dyadic.py) is ~0.07 s/element — too slow
# for an N² reaction. Reducing the α-integral analytically gives a single radial
# J0/J1/J2 Sommerfeld integral (~100× faster), which is what fnec's Rust
# `reflected_e_projected_fast` implements. This script validates that reduction
# against the 2-D oracle for all orientations.
#
# Reduced radial bracket B(λ) (ρ=√(ΔX²+ΔY²), φ=atan2(ΔY,ΔX), P=kz0/k0, Q=λ/k0,
# SS=s·o|xy, Dxx=sx·ox−sy·oy, Cxy=sx·oy+sy·ox):
#
#   E_proj = (k0·η0/8π²) ∫ (λ/kz0) e^{-j kz0 d} B(λ) dλ
#   B = R_TE·π[ SS·J0 + (Dxx·cos2φ + Cxy·sin2φ)·J2 ]
#     + R_TM·{ −P²π[ SS·J0 − (Dxx·cos2φ + Cxy·sin2φ)·J2 ]
#              − 2πj·P·Q·oz·J1·(sx cosφ + sy sinφ)
#              + 2πj·P·Q·sz·J1·(ox cosφ + oy sinφ)
#              + 2π·sz·oz·Q²·J0 }
#
# Reduces exactly to the shipped x/x φ=0 form (B/π = R_TE(J0+J2) − R_TM P²(J0−J2)).
# Result: matches the 2-D oracle to ~1e-6 for x/x, vertical, cross-pol, bent, tilted.

import sys
import numpy as np
from scipy.special import jv

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from general_dyadic import eproj_refl, k0, eta0, epsc, kg2, lam  # noqa: E402


def kz1_of(l):
    s = np.sqrt(kg2 - l * l + 0j)
    return np.where(s.imag > 0, -s, s)


def r_te(l, a, pec):
    return -1.0 + 0 * a if pec else (a - kz1_of(l)) / (a + kz1_of(l))


def r_tm(l, a, pec):
    if pec:
        return 1.0 + 0 * a
    b = kz1_of(l)
    return (epsc * a - b) / (epsc * a + b)


def eproj_1d(ds, do, dX, dY, d, pec=False, nr=4000):
    sx, sy, sz = ds
    ox, oy, oz = do
    rho, phi = np.hypot(dX, dY), np.arctan2(dY, dX)
    c2, s2, cf, sf = np.cos(2 * phi), np.sin(2 * phi), np.cos(phi), np.sin(phi)
    ss, dxx, cxy = sx * ox + sy * oy, sx * ox - sy * oy, sx * oy + sy * ox

    def radial(l, a):
        p, q = a / k0, l / k0
        j0, j1, j2 = jv(0, l * rho), jv(1, l * rho), jv(2, l * rho)
        te = r_te(l, a, pec) * np.pi * (ss * j0 + (dxx * c2 + cxy * s2) * j2)
        tm = r_tm(l, a, pec) * (
            -p * p * np.pi * (ss * j0 - (dxx * c2 + cxy * s2) * j2)
            - 2j * np.pi * p * q * oz * j1 * (sx * cf + sy * sf)
            + 2j * np.pi * p * q * sz * j1 * (ox * cf + oy * sf)
            + 2 * np.pi * sz * oz * q * q * j0
        )
        return (l / a) * np.exp(-1j * a * d) * (te + tm)

    th = np.linspace(1e-9, np.pi / 2 - 1e-9, nr)
    ip = np.trapezoid(radial(k0 * np.sin(th), k0 * np.cos(th)) * k0 * np.cos(th), th)
    tmax = np.arcsinh(45.0 / (k0 * d)) if d > 0 else 8.0
    tt = np.linspace(1e-9, tmax, nr)
    ie = np.trapezoid(radial(k0 * np.cosh(tt), -1j * k0 * np.sinh(tt)) * k0 * np.sinh(tt), tt)
    return (k0 * eta0 / (8 * np.pi * np.pi)) * (ip + ie)


def main():
    cases = [
        ("x/x on-axis", [1, 0, 0], [1, 0, 0], 0.3 * lam, 0.0),
        ("x/x off-axis", [1, 0, 0], [1, 0, 0], 0.2 * lam, 0.25 * lam),
        ("vertical z/z", [0, 0, 1], [0, 0, 1], 0.2 * lam, 0.0),
        ("cross x/z", [1, 0, 0], [0, 0, 1], 0.25 * lam, 0.1 * lam),
        ("bent x/y", [1, 0, 0], [0, 1, 0], 0.15 * lam, 0.2 * lam),
        ("tilted", list(np.array([1, 0, 1]) / 2 ** 0.5),
         list(np.array([0, 1, 1]) / 2 ** 0.5), 0.2 * lam, 0.15 * lam),
    ]
    print("=== 1-D reduction vs 2-D oracle (lossy ground) ===")
    for name, ds, do, dX, dY in cases:
        for hl in (0.1, 0.03):
            d = 2 * hl * lam
            a = eproj_1d(ds, do, dX, dY, d)
            b = eproj_refl(ds, do, dX, dY, d, na=192, nr=1500)
            print(f" {name:14s} h={hl}λ: rel={abs(a - b) / abs(b):.2e}")


if __name__ == "__main__":
    main()

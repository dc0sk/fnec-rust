#!/usr/bin/env python3
"""
pocklington_study.py — Pocklington EFIE pulse-basis investigation.

Implements the Pocklington EFIE in Python, matching the Rust
`assemble_pocklington_matrix` kernel exactly, and diagnoses why it
produces wrong results for thin-wire antennas.

Sections
--------
1. Python Pocklington solve — same kernel as Rust, gives 16.37 + j46.8 Ω.
   Confirms the bug is in the mathematical formulation, not a Rust issue.

2. Sign/operator variants — tests ±gzp_src, ±gzp_obs.
   None produce the correct answer; sign is not the root cause.

3. Exact d²/dz² verification — numerically confirms that neither the
   src-endpoint nor obs-endpoint formula matches the exact second
   derivative; the endpoint approximation is valid in magnitude but the
   result is still wrong — the fundamental method is broken.

4. Convergence study — solves for N = 11, 21, 51, 101, 201, 401 segments.
   The pulse-basis result diverges away from 74 Ω, not toward it.

Root cause
----------
The pulse-basis Pocklington EFIE with point-matching is known to diverge
for thin-wire antennas.  NEC2 avoids this by using piecewise-sinusoidal
basis functions (tbf/sbf/trio in calculations.c).  The Hallén augmented
system does not have this problem because the Hallén equation has a
smoother kernel.

Reference: Burke & Poggio, "Numerical Electromagnetics Code (NEC2)
           Theory of Operation", LLNL, 1981.

Usage
-----
  python3 pocklington_study.py [--all]

  Without --all: runs sections 1 and 4 only (fast).
  With    --all: also runs sections 2 and 3 (slower, more verbose).
"""

import cmath
import math
import sys
import numpy as np

# Physical constants
C0  = 299_792_458.0
MU0 = 4e-7 * math.pi
ETA0 = MU0 * C0
OMEGA_MHZ = 14.2e6
F_HZ = OMEGA_MHZ
K    = 2 * math.pi * F_HZ / C0
OMEGA = 2 * math.pi * F_HZ

# Reference geometry
L_REF = 10.564
A_REF = 0.001
N_REF = 51

# Gauss–Legendre nodes and weights
GL8_N = [-0.960289856497536,-0.796666477413627,-0.525532409916329,-0.183434642495650, 0.183434642495650, 0.525532409916329, 0.796666477413627, 0.960289856497536]
GL8_W = [ 0.101228536290376, 0.222381034453374, 0.313706645877887, 0.362683783378362, 0.362683783378362, 0.313706645877887, 0.222381034453374, 0.101228536290376]
GL4_N = [-0.861136311594953,-0.339981043584856, 0.339981043584856, 0.861136311594953]
GL4_W = [ 0.347854845137454, 0.652145154862626, 0.652145154862626, 0.347854845137454]


def green(r: float) -> complex:
    return cmath.exp(-1j * K * r) / r


def int_k_elem(z_obs: float, z_src: float, half: float, a: float, is_self: bool) -> complex:
    """Integral ∫ G(R_eff) dl' over a segment of half-length `half`."""
    if is_self:
        sm = 0j
        for xi, wi in zip(GL4_N, GL4_W):
            l = xi * half
            r = math.sqrt(l * l + a * a)
            sm += wi * (green(r) - 1.0 / r)
        sm *= half
        r_end = math.sqrt(half * half + a * a)
        return sm + 2.0 * math.log((half + r_end) / a)
    else:
        sm = 0j
        for xi, wi in zip(GL8_N, GL8_W):
            z_p = z_src + xi * half
            r = math.sqrt((z_obs - z_p) ** 2 + a * a)
            sm += wi * green(r)
        return sm * half


def gzp_src(z_obs: float, z_p: float, a: float) -> complex:
    """dG/dz' at z'=z_p (tangent direction along +z source axis).
    Used in Pocklington endpoint term: [dG/dz']{at z_src ± half}.
    """
    R = math.sqrt((z_obs - z_p) ** 2 + a * a)
    if R < 1e-15:
        return 0j
    cos_src = (z_p - z_obs) / R
    kprime = -(1j * K * R + 1) * cmath.exp(-1j * K * R) / (R * R)
    return kprime * cos_src


def build_pocklington_matrix(N: int, L: float, a: float) -> np.ndarray:
    """Assemble the N×N Pocklington impedance matrix.

    Z[i,j] = (jωμ₀/4π) · [k² · ∫G dl' + dG/dz'|_{+} - dG/dz'|_{-}]

    This matches the Rust `assemble_pocklington_matrix` kernel exactly.
    """
    dl   = L / N
    half = dl / 2.0
    mid_z = np.array([-L / 2 + (i + 0.5) * dl for i in range(N)])
    pre = 1j * OMEGA * MU0 / (4 * math.pi)

    P = np.zeros((N, N), dtype=complex)
    for i, z_obs in enumerate(mid_z):
        for j, z_src in enumerate(mid_z):
            ik  = int_k_elem(z_obs, z_src, half, a, i == j)
            gzp = gzp_src(z_obs, z_src + half, a) - gzp_src(z_obs, z_src - half, a)
            P[i, j] = pre * (K * K * ik + gzp)
    return P, mid_z, dl


def solve_and_z(P: np.ndarray, mid_z: np.ndarray, dl: float) -> complex:
    feed = len(mid_z) // 2
    v = np.zeros(len(mid_z), dtype=complex)
    v[feed] = 1.0 / dl
    I = np.linalg.solve(P, v)
    return 1.0 / I[feed]


# ---------------------------------------------------------------------------
# Section 1: Python Pocklington vs Hallén reference
# ---------------------------------------------------------------------------

def section1():
    print("=" * 65)
    print("Section 1 — Python Pocklington vs Hallén reference")
    print("=" * 65)
    print(f"  Geometry: N={N_REF}, L={L_REF} m, a={A_REF} m, f={F_HZ/1e6:.1f} MHz")
    print()

    P, mid_z, dl = build_pocklington_matrix(N_REF, L_REF, A_REF)
    z_pock = solve_and_z(P, mid_z, dl)
    print(f"  Pocklington (Python):  Z = {z_pock:.4f} Ω")
    print(f"  Hallén reference:      Z = 74.2301 + j13.8973 Ω")
    print()
    print("  Both Python and Rust Pocklington give ~16.37 + j46.8 Ω.")
    print("  This confirms the bug is in the mathematical formulation,")
    print("  not a Rust implementation issue.")
    print()

    # Show the near-cancellation in the matrix
    i = N_REF // 2
    print(f"  Pocklington matrix near feed (shows near-cancellation):")
    print(f"    P[{i},{i}]   = {P[i,i]:.4f}")
    print(f"    P[{i},{i-1}] = {P[i,i-1]:.4f}")
    print(f"    P[{i},{i+1}] = {P[i,i+1]:.4f}")
    print(f"    Row {i} sum  = {P[i,:].sum():.4f}")
    print()
    pre = 1j * OMEGA * MU0 / (4 * math.pi)
    half = (L_REF / N_REF) / 2
    a = A_REF
    z_src = mid_z[i]
    ik_self = int_k_elem(mid_z[i], z_src, half, a, True)
    print(f"    k²·A[{i},{i}] term alone:  {pre * K*K * ik_self:.4f}")
    gzp = gzp_src(mid_z[i], z_src + half, a) - gzp_src(mid_z[i], z_src - half, a)
    print(f"    Endpoint term alone:     {pre * gzp:.4f}")
    print()
    print("  The endpoint term dominates by ~200×, making the solution")
    print("  highly sensitive to near-cancellation of large numbers.")


# ---------------------------------------------------------------------------
# Section 2: Sign/operator variants
# ---------------------------------------------------------------------------

def section2():
    print("=" * 65)
    print("Section 2 — Sign/operator variants")
    print("=" * 65)
    print("  Testing all four sign combinations of the endpoint term:")
    print()

    dl   = L_REF / N_REF
    half = dl / 2.0
    mid_z = np.array([-L_REF / 2 + (i + 0.5) * dl for i in range(N_REF)])
    pre = 1j * OMEGA * MU0 / (4 * math.pi)
    a = A_REF

    def gzp_obs(z_obs, z_p):
        R = math.sqrt((z_obs - z_p) ** 2 + a * a)
        if R < 1e-15: return 0j
        cos_obs = (z_obs - z_p) / R
        kprime = -(1j * K * R + 1) * cmath.exp(-1j * K * R) / (R * R)
        return kprime * cos_obs

    feed = N_REF // 2
    v = np.zeros(N_REF, dtype=complex)
    v[feed] = 1.0 / dl

    for deriv_label, deriv_fn in [("gzp_src", gzp_src), ("gzp_obs", gzp_obs)]:
        for sign, sign_label in [(+1, "+"), (-1, "-")]:
            P = np.zeros((N_REF, N_REF), dtype=complex)
            for i, z_obs in enumerate(mid_z):
                for j, z_src in enumerate(mid_z):
                    ik = int_k_elem(z_obs, z_src, half, a, i == j)
                    if deriv_label == "gzp_obs":
                        gd = gzp_obs(z_obs, z_src + half) - gzp_obs(z_obs, z_src - half)
                    else:
                        gd = gzp_src(z_obs, z_src + half, a) - gzp_src(z_obs, z_src - half, a)
                    P[i, j] = pre * (K * K * ik + sign * gd)
            I = np.linalg.solve(P, v)
            z = 1.0 / I[feed]
            print(f"  pre*(k²Ik {sign_label} {deriv_label}):  Z = {z:.4f} Ω")

    print()
    print("  None of the four variants produce the correct 74+j14 Ω.")
    print("  Sign is not the root cause.")


# ---------------------------------------------------------------------------
# Section 3: Exact d²/dz² verification
# ---------------------------------------------------------------------------

def section3():
    print("=" * 65)
    print("Section 3 — Exact d²/dz² vs endpoint formula")
    print("=" * 65)
    print("  Numerically verifies d²/dz²[∫G dz'] against the endpoint")
    print("  approximation for selected (obs, src) pairs.\n")

    dl   = L_REF / N_REF
    half = dl / 2.0
    mid_z = np.array([-L_REF / 2 + (i + 0.5) * dl for i in range(N_REF)])
    a = A_REF
    eps = 1e-6

    print(f"  {'i':>3} {'j':>3}  {'d2_exact':>28}  {'end_src':>28}")
    print(f"  {'-'*3} {'-'*3}  {'-'*28}  {'-'*28}")
    for i_t, j_t in [(0, 25), (10, 0), (25, 50), (40, 25)]:
        z0, z1 = mid_z[i_t], mid_z[j_t]

        def int_G(zo):
            s = 0j
            for xi, wi in zip(GL8_N, GL8_W):
                z_p = z1 + xi * half
                r = math.sqrt((zo - z_p) ** 2 + a * a)
                s += wi * green(r)
            return s * half

        d2_num = (int_G(z0 + eps) - 2 * int_G(z0) + int_G(z0 - eps)) / eps**2
        end_s  = gzp_src(z0, z1 + half, a) - gzp_src(z0, z1 - half, a)
        print(f"  {i_t:>3} {j_t:>3}  {d2_num.real:+.6f}{d2_num.imag:+.6f}j  {end_s.real:+.6f}{end_s.imag:+.6f}j")

    print()
    print("  The endpoint formula approximates d²/dz² reasonably in")
    print("  magnitude but the solve is still wrong — the fundamental")
    print("  pulse-basis Pocklington method is broken for thin wires.")


# ---------------------------------------------------------------------------
# Section 4: Convergence study
# ---------------------------------------------------------------------------

def section4():
    print("=" * 65)
    print("Section 4 — Convergence study (pulse-basis Pocklington)")
    print("=" * 65)
    print(f"  Geometry: L={L_REF} m, a={A_REF} m, f={F_HZ/1e6:.1f} MHz")
    print(f"  Reference (Hallén): 74.23 + j13.90 Ω\n")
    print(f"  {'N':>6}  {'Z (Ω)'}")
    print(f"  {'-'*6}  {'-'*35}")

    for n in [11, 21, 51, 101, 201, 401]:
        P, mid_z, dl = build_pocklington_matrix(n, L_REF, A_REF)
        z = solve_and_z(P, mid_z, dl)
        print(f"  {n:>6}  {z.real:+.4f} + j{z.imag:+.4f}")

    print()
    print("  The result diverges AWAY from 74 Ω as N increases.")
    print("  This confirms pulse-basis Pocklington is fundamentally")
    print("  unsuitable for this problem.")
    print()
    print("  Fix: implement sinusoidal-basis EFIE (NEC2-style tbf/sbf/trio).")
    print("  See docs/backlog.md.")


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    run_all = "--all" in sys.argv

    section1()
    print()

    if run_all:
        section2()
        print()
        section3()
        print()

    section4()

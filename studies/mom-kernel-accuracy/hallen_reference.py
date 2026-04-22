#!/usr/bin/env python3
"""
hallen_reference.py — Independent Hallén MoM reference solver.

Implements the augmented Hallén integral equation for a centre-fed
half-wave dipole using:
  - 8-point Gauss–Legendre quadrature for off-diagonal elements
  - 4-point GL for the self-term smooth part (singularity subtracted)
  - Analytic log term for the near-singularity
  - Augmented system: [A | -cos(kz)] [I; C_hom] = b
    with endpoint constraints I[0] = I[N-1] = 0

Ground-truth result for the benchmark geometry: 74.23 + j13.90 Ω

Benchmark geometry
------------------
  N = 51 segments, L = 10.564 m, a = 0.001 m, f = 14.2 MHz
  Centre segment: feed = 25 (0-based), tag=1, seg=26

Usage
-----
  python3 hallen_reference.py
"""

import cmath
import math
import numpy as np

# --- Physical constants ---------------------------------------------------
C0  = 299_792_458.0          # speed of light, m/s
MU0 = 4e-7 * math.pi         # permeability of free space, H/m
ETA0 = MU0 * C0              # wave impedance ≈ 376.73 Ω

# --- Benchmark geometry ---------------------------------------------------
F_HZ = 14.2e6
L    = 10.564      # total dipole length, m
A    = 0.001       # wire radius, m
N    = 51          # number of segments

# --- Derived quantities ---------------------------------------------------
K    = 2 * math.pi * F_HZ / C0
DL   = L / N
HALF = DL / 2.0
MID_Z = np.array([-L / 2 + (i + 0.5) * DL for i in range(N)])
FEED  = N // 2     # index 25 for N=51

# --- Gauss–Legendre nodes and weights ------------------------------------
GL8_N = [
    -0.960289856497536, -0.796666477413627,
    -0.525532409916329, -0.183434642495650,
     0.183434642495650,  0.525532409916329,
     0.796666477413627,  0.960289856497536,
]
GL8_W = [
    0.101228536290376, 0.222381034453374,
    0.313706645877887, 0.362683783378362,
    0.362683783378362, 0.313706645877887,
    0.222381034453374, 0.101228536290376,
]

GL4_N = [
    -0.861136311594953, -0.339981043584856,
     0.339981043584856,  0.861136311594953,
]
GL4_W = [
    0.347854845137454, 0.652145154862626,
    0.652145154862626, 0.347854845137454,
]


def green(r: float) -> complex:
    """Free-space scalar Green's function G(r) = e^{-jkr} / r."""
    return cmath.exp(-1j * K * r) / r


def build_a_matrix() -> np.ndarray:
    """Assemble the N×N Hallén A-matrix."""
    A_mat = np.zeros((N, N), dtype=complex)
    for i, z_obs in enumerate(MID_Z):
        for j, z_src in enumerate(MID_Z):
            if i == j:
                # Self-term: singularity subtraction + analytic log
                smooth = 0j
                for xi, wi in zip(GL4_N, GL4_W):
                    l = xi * HALF
                    r = math.sqrt(l * l + A * A)
                    smooth += wi * (green(r) - 1.0 / r)
                smooth *= HALF
                r_end = math.sqrt(HALF * HALF + A * A)
                A_mat[i, j] = smooth + 2.0 * math.log((HALF + r_end) / A)
            else:
                # Off-diagonal: 8-point GL with reduced kernel
                s = 0j
                for xi, wi in zip(GL8_N, GL8_W):
                    z_p = z_src + xi * HALF
                    r = math.sqrt((z_obs - z_p) ** 2 + A * A)
                    s += wi * green(r)
                A_mat[i, j] = s * HALF
    return A_mat


def build_hallen_rhs() -> np.ndarray:
    """Build the Hallén RHS vector b_m = -j·(2π/η₀)·sin(k·|z_m|).

    This corresponds to a unit-voltage (V=1 V) delta-gap source at
    the feed segment, after the Hallen integral equation derivation.
    """
    scale = 2.0 * math.pi / ETA0
    return np.array(
        [-1j * scale * math.sin(K * abs(z)) for z in MID_Z],
        dtype=complex,
    )


def solve_hallen(a_mat: np.ndarray, rhs: np.ndarray):
    """Solve the augmented Hallén system [A | -cos] [I; C] = b
    with endpoint constraints I[0] = I[N-1] = 0.

    Returns (currents, c_hom).
    """
    cos_vec = np.cos(K * MID_Z)

    # Augmented system: (N+2) equations, (N+1) unknowns [I_0..I_{N-1}, C_hom]
    rows, cols = N + 2, N + 1
    M = np.zeros((rows, cols), dtype=complex)
    y = np.zeros(rows, dtype=complex)

    M[:N, :N] = a_mat
    M[:N, N]  = -cos_vec
    y[:N]      = rhs

    # Endpoint current = 0
    M[N,   0]   = 1.0
    M[N+1, N-1] = 1.0

    x = np.linalg.lstsq(M, y, rcond=None)[0]
    return x[:N], x[N]


def main():
    print(f"Hallén reference solver — {N}-segment dipole at {F_HZ/1e6:.1f} MHz")
    print(f"  L = {L} m, a = {A} m, dl = {DL:.6f} m, k = {K:.6f} rad/m")

    a_mat = build_a_matrix()
    rhs   = build_hallen_rhs()
    I, c_hom = solve_hallen(a_mat, rhs)

    z_feed = 1.0 / I[FEED]
    print(f"\n  Feed current I[{FEED}] = {I[FEED]:.6e}")
    print(f"  Impedance Z  = {z_feed:.6f} Ω")
    print(f"\n  Expected: 74.23 + j13.90 Ω  (matches xnec2c NEC2 reference)")

    # Print current distribution summary
    print(f"\n  Current distribution (normalised to max):")
    i_max = np.max(np.abs(I))
    for idx in [0, N//4, FEED, 3*N//4, N-1]:
        print(f"    seg[{idx:2d}]: |I| = {abs(I[idx])/i_max:.4f}  arg = {cmath.phase(I[idx])*180/math.pi:+.2f}°")


if __name__ == "__main__":
    main()

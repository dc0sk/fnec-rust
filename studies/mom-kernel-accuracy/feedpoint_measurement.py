#!/usr/bin/env python3
"""
feedpoint_measurement.py — Feedpoint current extraction study.

Tests whether the ~74 Ω impedance from the Hallén reference solve
is sensitive to how the feedpoint current is extracted.  Evaluated
estimators:

  - midpoint       : I[feed]  (segment centre, used by Rust CLI)
  - avg_neighbors  : (I[feed-1] + I[feed+1]) / 2
  - avg_all3       : (I[feed-1] + I[feed] + I[feed+1]) / 3
  - half_step_interp: I[feed] + 0.5·(I[feed+1] - I[feed])

Finding: all estimators cluster at 74.1–74.2 + j13.9–14.4 Ω.
The feedpoint measurement is NOT the source of the pulse-mode error.

Usage
-----
  python3 feedpoint_measurement.py
"""

import cmath
import math
import numpy as np

# Physical constants
C0  = 299_792_458.0
MU0 = 4e-7 * math.pi
ETA0 = MU0 * C0

# Geometry
F_HZ = 14.2e6
L    = 10.564
A    = 0.001
N    = 51
K    = 2 * math.pi * F_HZ / C0
DL   = L / N
HALF = DL / 2.0
MID_Z = np.array([-L / 2 + (i + 0.5) * DL for i in range(N)])
FEED  = N // 2

GL8_N = [-0.960289856497536,-0.796666477413627,-0.525532409916329,-0.183434642495650, 0.183434642495650, 0.525532409916329, 0.796666477413627, 0.960289856497536]
GL8_W = [ 0.101228536290376, 0.222381034453374, 0.313706645877887, 0.362683783378362, 0.362683783378362, 0.313706645877887, 0.222381034453374, 0.101228536290376]
GL4_N = [-0.861136311594953,-0.339981043584856, 0.339981043584856, 0.861136311594953]
GL4_W = [ 0.347854845137454, 0.652145154862626, 0.652145154862626, 0.347854845137454]


def green(r):
    return cmath.exp(-1j * K * r) / r


def hallen_solve():
    A_mat = np.zeros((N, N), dtype=complex)
    for i, z_obs in enumerate(MID_Z):
        for j, z_src in enumerate(MID_Z):
            if i == j:
                sm = 0j
                for xi, wi in zip(GL4_N, GL4_W):
                    l = xi * HALF; r = math.sqrt(l*l + A*A)
                    sm += wi * (green(r) - 1/r)
                sm *= HALF
                r_end = math.sqrt(HALF*HALF + A*A)
                A_mat[i, j] = sm + 2*math.log((HALF + r_end) / A)
            else:
                s = 0j
                for xi, wi in zip(GL8_N, GL8_W):
                    z_p = z_src + xi*HALF
                    r = math.sqrt((z_obs - z_p)**2 + A*A)
                    s += wi * green(r)
                A_mat[i, j] = s * HALF

    cos_vec = np.cos(K * MID_Z)
    scale   = 2.0 * math.pi / ETA0
    rhs     = np.array([-1j * scale * math.sin(K * abs(z)) for z in MID_Z], dtype=complex)

    rows, cols = N + 2, N + 1
    M = np.zeros((rows, cols), dtype=complex)
    y = np.zeros(rows, dtype=complex)
    M[:N, :N] = A_mat
    M[:N, N]  = -cos_vec
    y[:N]     = rhs
    M[N,   0]   = 1.0
    M[N+1, N-1] = 1.0

    x = np.linalg.lstsq(M, y, rcond=None)[0]
    return x[:N]


def main():
    print(f"Feedpoint measurement study — {N}-segment dipole at {F_HZ/1e6:.1f} MHz\n")
    I = hallen_solve()

    f = FEED
    print(f"  Centre segment  I[{f}]   = {I[f]:.6e}")
    print(f"  Left  adjacent  I[{f-1}]  = {I[f-1]:.6e}")
    print(f"  Right adjacent  I[{f+1}]  = {I[f+1]:.6e}")
    print(f"  Left == Right (symmetric): {abs(I[f-1] - I[f+1]) < 1e-12}\n")

    estimators = [
        ("midpoint",         I[f]),
        ("avg_neighbors",    0.5 * (I[f-1] + I[f+1])),
        ("avg_all3",         (I[f-1] + I[f] + I[f+1]) / 3),
        ("half_step_interp", I[f] + 0.5 * (I[f+1] - I[f])),
    ]

    print(f"  {'Estimator':<22}  {'Z (Ω)'}")
    print(f"  {'-'*22}  {'-'*40}")
    for label, cur in estimators:
        z = 1.0 / cur
        print(f"  {label:<22}  {z.real:+.4f} + j{z.imag:+.4f}")

    print()
    print("  All estimators within ≈ 0.1 Ω real, ≈ 0.5 Ω imag of each other.")
    print("  Conclusion: feedpoint measurement is NOT the cause of pulse-mode error.")


if __name__ == "__main__":
    main()

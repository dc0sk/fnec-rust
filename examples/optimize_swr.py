#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# optimize_swr.py — find the half-element length of a 14.2 MHz dipole that
# minimises SWR into a 50 Ω feedline, using fnec as the solver backend.
#
# Requirements:
#   - Python >= 3.8
#   - fnec on PATH  (cargo build --release && export PATH=$PATH:./target/release)
#
# Run:
#   python3 examples/optimize_swr.py
#
# See docs/automation-guide.md § 6 for a detailed walkthrough.

import json
import math
import subprocess
import sys

FREQ_MHZ = 14.2
Z0 = 50.0          # reference impedance (Ω)
WIRE_RADIUS = 0.001  # metres
SEGMENTS = 51
# feedpoint is the centre segment: (SEGMENTS + 1) // 2
FEEDPOINT_SEG = (SEGMENTS + 1) // 2


def build_deck(half_len: float) -> str:
    """Return a NEC deck string for a symmetric dipole with given half-length."""
    return (
        f"CM Dipole optimiser — half_len={half_len:.6f} m\n"
        f"CE\n"
        f"GW 1 {SEGMENTS} 0 0 -{half_len:.6f} 0 0 {half_len:.6f} {WIRE_RADIUS}\n"
        f"GE 0\n"
        f"EX 0 1 {FEEDPOINT_SEG} 0 1.0 0.0\n"
        f"FR 0 1 0 0 {FREQ_MHZ} 0.0\n"
        f"EN\n"
    )


def solve(half_len: float) -> dict:
    """Run fnec and return the first impedance record as a dict."""
    deck = build_deck(half_len)
    proc = subprocess.run(
        ["fnec", "--output-format", "json", "/dev/stdin"],
        input=deck,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        stderr = proc.stderr.strip()
        raise RuntimeError(
            f"fnec exited with code {proc.returncode} for half_len={half_len:.6f}:\n{stderr}"
        )
    records = json.loads(proc.stdout)
    if not records:
        raise RuntimeError(
            f"fnec returned empty JSON for half_len={half_len:.6f} — "
            "deck may be missing an EX or FR card"
        )
    return records[0]


def swr(rec: dict) -> float:
    """Compute SWR from an impedance record."""
    z = complex(rec["z_re"], rec["z_im"])
    gamma = (z - Z0) / (z + Z0)
    rho = abs(gamma)
    if rho >= 1.0:
        return float("inf")
    return (1.0 + rho) / (1.0 - rho)


def swr_at(half_len: float, *, verbose: bool = False) -> float:
    """Evaluate SWR at a given half-length, with optional verbose output."""
    rec = solve(half_len)
    s = swr(rec)
    if verbose:
        z_re, z_im = rec["z_re"], rec["z_im"]
        sign = "+" if z_im >= 0 else "-"
        print(
            f"    half_len={half_len:.4f} m  "
            f"z=({z_re:.2f}{sign}j{abs(z_im):.2f})Ω  "
            f"SWR={s:.3f}"
        )
    return s


def golden_search(f, lo: float, hi: float, tol: float = 1e-3) -> tuple[float, int]:
    """
    Minimise f on [lo, hi] using golden-section search.
    Returns (best_x, iterations).
    """
    phi = (math.sqrt(5.0) - 1.0) / 2.0
    c = hi - phi * (hi - lo)
    d = lo + phi * (hi - lo)
    fc, fd = f(c), f(d)
    iterations = 0
    while abs(hi - lo) > tol:
        iterations += 1
        if fc < fd:
            hi = d
            d, fd = c, fc
            c = hi - phi * (hi - lo)
            fc = f(c)
        else:
            lo = c
            c, fc = d, fd
            d = lo + phi * (hi - lo)
            fd = f(d)
    return (lo + hi) / 2.0, iterations


def main() -> int:
    print(f"fnec optimize_swr.py — find dipole half-length for minimum SWR at {FREQ_MHZ} MHz")
    print(f"reference impedance Z0 = {Z0:.0f} Ω\n")

    lo, hi = 4.5, 6.0

    print(f"Scanning initial bracket [lo={lo:.2f}, hi={hi:.2f}]:")
    swr_at(lo, verbose=True)
    swr_at(hi, verbose=True)
    print()

    iteration_count = [0]

    def tracked(x: float) -> float:
        iteration_count[0] += 1
        s = swr_at(x, verbose=True)
        return s

    print(f"Golden-section search (tol=1e-3):")
    best_len, iters = golden_search(tracked, lo, hi, tol=1e-3)
    print(f"\nconverged in {iters} search iterations ({iteration_count[0]} total solver calls)")

    # Final evaluation at the converged point
    rec = solve(best_len)
    best_swr = swr(rec)
    print(f"\nResult:")
    print(f"  optimal half-length: {best_len:.4f} m")
    print(f"  z_re: {rec['z_re']:.2f} Ω  z_im: {rec['z_im']:.2f} Ω")
    print(f"  SWR: {best_swr:.3f}  (Z0={Z0:.0f} Ω)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

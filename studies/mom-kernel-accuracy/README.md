---
study: MoM kernel accuracy investigation
branch: fix/mom-kernel-accuracy
date: 2026-04-22
author: DC0SK
---

# MoM Kernel Accuracy — Python Studies

These scripts were written during the `fix/mom-kernel-accuracy` debugging
session to validate the Rust solver against an independent Python reference
and to diagnose why the Pocklington/pulse solver mode produces wrong results.

## Scripts

| File | Purpose |
|------|---------|
| `hallen_reference.py` | Reference Hallén MoM solver (8-pt GL, singularity subtraction). Ground-truth result: **74.23 + j13.90 Ω** |
| `feedpoint_measurement.py` | Verifies that all feedpoint estimators (midpoint, neighbor avg, interpolated) agree with the reference, ruling out measurement error as the cause of pulse-mode divergence |
| `pocklington_study.py` | Implements the Pocklington EFIE in Python matching the Rust kernel exactly, tests sign/operator variants, and performs a convergence study that confirms pulse-basis Pocklington diverges for thin-wire antennas |

## Benchmark geometry

- 51-segment half-wave dipole at 14.2 MHz
- Total length L = 10.564 m, wire radius a = 0.001 m
- Centre-fed (EX 0 tag=1 seg=26)
- Free-space, PEC wire

## Key findings

1. **Hallén solver is correct.** The augmented Hallén system produces
   `74.23 + j13.90 Ω`, matching the Python reference exactly.

2. **Feedpoint measurement is not the issue.** All estimators cluster at
   `74.1–74.2 + j13.9–14.4 Ω`.

3. **Pulse-basis Pocklington diverges.** The Python convergence study
   (N = 11 … 401) shows the result drifting away from the correct
   answer, not toward it. NEC2 avoids this by using piecewise-sinusoidal
   basis functions (`tbf`/`sbf`/`trio` in `calculations.c`).

## Next step

Implement sinusoidal-basis EFIE matrix assembly to fix pulse/continuity modes.
Tracked in `docs/backlog.md`.

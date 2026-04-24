---
project: fnec-rust
doc: docs/solver-findings.md
status: living
last_updated: 2026-04-24
---

# Solver Findings

## Scope

This document captures findings from feedpoint-impedance investigations for a
center-fed half-wave dipole test case at 14.2 MHz.

## Test geometry

- Wire: single GW, length 10.564 m, radius 0.001 m, frequency 14.2 MHz
- Segments: 51 (primary benchmark), also swept at 11, 21, 101, 201, 401
- Excitation: EX 0, center segment (tag=1, seg=26), 1.0 V
- Reference: xnec2c NEC2 C implementation; Python MoM scripts in `studies/mom-kernel-accuracy/`

## Confirmed results (2026-04-22)

### Hallén solver — CORRECT

After fixing two bugs (`e098fb4`, `c302f29`), the Hallén augmented system is now:

$$\left[\,A\;|\;-\cos\,\right]
\begin{bmatrix}
I\\
C
\end{bmatrix}
= b$$

with the correct RHS prefactor:

$$\text{Hallén RHS prefactor} = \frac{2\pi}{\eta_0}$$

Validation results:

| Mode | N=51 | Python reference |
|:-----|:-----|:----------------|
| hallen | **74.242874 + j13.899516 Ω** | 74.23 + j13.90 Ω ✓ |

$$Z_{\mathrm{hallen}}(N=51) \approx 74.242874 + j\,13.899516\,\Omega$$

The Hallén augmented system (`[A | −cos] [I; C] = b`) with the correct
`2π/η₀` RHS prefactor and NEC sign convention is the production-accurate solver.

### Hallén with GN=1 (PEC image method) — REGRESSION-COVERED

For `corpus/dipole-ground-51seg.nec` (14.2 MHz, 10 m AGL), current CI-regression value is:

| Mode | Case | Value |
|:-----|:-----|:------|
| hallen + GN=1 | dipole-ground-51seg | **81.914743 + j16.416629 Ω** |

$$Z_{\mathrm{hallen},\,GN=1} \approx 81.914743 + j\,16.416629\,\Omega$$

This confirms GN=1 ground behavior is no longer silently ignored in the Hallen path.
External-reference parity for this case is still pending explicit xnec2c/4nec2 capture;
the corpus now tracks an `external_reference_candidate` placeholder for that follow-up.

### Pulse/continuity solver — DIVERGES (known broken)

Pulse-basis Pocklington EFIE diverges from the physical solution as segment
count increases.  This is a fundamental property of the method, not a bug in
the implementation:

| N | Z_pulse |
|---|---------|
| 11 | 264.6 + j82.7 Ω |
| 21 | 42.2 + j88.9 Ω |
| 51 | 16.4 + j46.8 Ω |
| 101 | 11.6 + j32.1 Ω |
| 201 | 9.4 + j22.0 Ω |
| 401 | 8.1 + j14.1 Ω |

Root cause: the endpoint-derivative terms dominate the self-impedance element
(~200× larger than the k²∫G term), causing heavy near-cancellation that
amplifies discretisation error.  NEC2 avoids this by using piecewise-sinusoidal
basis functions (`tbf`/`sbf`/`trio` in `calculations.c`).

The pulse/continuity modes are marked **experimental** in the CLI with a
runtime warning.  A sinusoidal-basis EFIE fix is tracked in `docs/backlog.md`.

## Key bugs fixed on this branch

| Commit | Fix |
|--------|-----|
| `e098fb4` | Hallén RHS missing `2π/η₀` prefactor — was using `j·k` alone |
| `c302f29` | Pulse RHS sign wrong — was `+v/λ`, correct is `−v/λ` (NEC sign convention) |

## What did NOT cause the pulse divergence

- Feedpoint measurement method (all estimators agree within ≈ 0.5 Ω — see `feedpoint_measurement.py`)
- RHS sign/scaling (all four ± sign variants of the endpoint term give similarly wrong results)
- Numerical precision of the endpoint derivative (exact d²/dz² via finite differences gives the same answer)

## Practical lessons

- Keep experiments reproducible using gitignored temp folders and `studies/` scripts.
- Separate physics-formulation changes from reporting/output changes to isolate regressions.
- When a solver gives wrong results, verify it in Python before modifying Rust — it is faster to falsify a formulation hypothesis in 10 lines of Python than in a Rust edit–compile–run cycle.

## External references

- xnec2c source: https://github.com/KJ7LNW/xnec2c (primary NEC2 C reference)
- Burke & Poggio, "NEC2 Theory of Operation", LLNL 1981
- M5AIQ NEC resources: https://www.qsl.net/m5aiq/nec.html


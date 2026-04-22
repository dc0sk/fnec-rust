---
project: fnec-rust
doc: docs/solver-findings.md
status: living
last_updated: 2026-04-23
---

# Solver Findings

## Scope

This document captures recent findings from feedpoint-impedance investigations for a center-fed half-wave dipole test case at 14.2 MHz.

## Test geometry

- Wire: single GW, length 10.564 m, radius 0.001 m
- Frequency: 14.2 MHz
- Typical segmentation explored: 11, 21, 51, 101, 201+
- Excitation: center segment, 1.0 V complex source

## Key findings

1. Excitation normalization matters:
- For point-matched EFIE, the driven RHS should be field-like at the match point.
- Using source voltage directly instead of proper normalization can produce a scaled Z_in error.

2. Input-impedance reporting must use source voltage, not field term:
- Correct feedpoint impedance is Z_in = V_source / I_source.
- If RHS stores V/dl, then V_source = (V/dl) * dl at the driven segment.

3. Pocklington pulse-basis behavior is fragile at low-to-moderate segment counts:
- Endpoint-derivative terms are large and create heavy cancellation.
- Conditioning worsens and practical convergence to physically expected dipole impedance can be slow.

4. Hallen path is sensitive to equation convention details:
- RHS scaling, homogeneous-term sign, and endpoint constraints strongly affect results.
- Several formulations can numerically solve but still produce non-physical feedpoint impedance.

5. Current status:
- The previous Hallen experiment path is informative but not yet production-ready for parity goals.
- Next direction is continuity-enforcing basis support (rooftop/sinusoidal-like behavior).

## Current mode benchmarks (2026-04-23)

Solver modes were swept at 14.2 MHz for N = 11, 21, 51, 101 segments using the same center-fed dipole geometry.

| Mode | N=11 | N=21 | N=51 | N=101 |
|:--|:--|:--|:--|:--|
| hallen | 363.667 - j629.119 | 422.921 - j196.313 | 466.482 + j87.333 | 482.832 + j187.315 |
| pulse | 13.030 - j0.166 | 6.153 - j0.037 | 2.156 - j0.005 | 0.942 - j0.001 |
| continuity | 13.891 - j0.031 | 6.337 - j0.006 | 2.178 - j0.001 | 0.946 - j0.000 |

Interpretation:

- Hallen path in the current implementation does not trend toward expected half-wave dipole feedpoint values.
- Pulse and continuity paths still collapse toward near-zero resistance as segmentation increases.
- Even after separating Hallen and Pocklington assembly call paths in the CLI, observed feedpoint trends are unchanged for this dipole case, indicating unresolved scaling/conditioning issues beyond simple mode routing.
- Using NEC2-style pulse RHS wavelength normalization guidance (voltage source term proportional to 1/(dl*lambda)) did not materially change the dipole feedpoint trend in this implementation.
- The continuity transform infrastructure remains useful groundwork, but physically correct parity still requires additional formulation correction and validation.

## Practical lessons

- Keep experiments reproducible and local to the repo using a gitignored temp folder.
- Favor incremental solver refactors with testable intermediate layers (basis transform, constraints, solve path).
- Separate physics-formulation changes from reporting/output changes to isolate regressions.

## External references considered

- xnec2c (primary parity reference corpus source)
- M5AIQ NEC resources: https://www.qsl.net/m5aiq/nec.html
- yeti01/nec2: https://github.com/yeti01/nec2
- tmolteno/necpp: https://github.com/tmolteno/necpp

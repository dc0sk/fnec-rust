---
project: fnec-rust
doc: docs/rooftop-basis-plan.md
status: living
last_updated: 2026-04-24
---

# Rooftop Basis Plan

## Goal

Move from pulse-only current representation to a continuity-enforcing basis (rooftop/triangular-style transform) to improve physical current behavior and feedpoint-impedance convergence.

## Why this is the next step

Recent findings showed that pulse-only formulations can produce strong cancellation and conditioning problems in thin-wire dipole tests. A continuity basis is the next practical step toward stable NEC-like behavior.

## Phase 1: Basis transform layer

1. Implement a basis-transform utility T for each straight wire chain.
2. Ensure endpoint current constraints are intrinsic (tip current = 0).
3. Provide reversible mapping APIs:
- segment_current_from_basis(a)
- basis_from_segment_current(I) (least-squares helper)

## Phase 2: Matrix solve integration

1. Keep existing matrix assembly initially.
2. Solve transformed system Z*T*a = v.
3. Recover segment currents I = T*a for reporting.
4. Validate against existing unit tests and add regression tests for transform consistency.

## Phase 3: Numerical robustness

1. Replace normal-equation fallback where possible with QR-based solve.
2. Add conditioning diagnostics (matrix norm, pivot floor, residual).
3. Add deterministic tolerances for CI comparisons.

## Phase 4: Physics validation

1. Re-run the half-wave dipole benchmark across segment counts.
2. Track convergence of real and imaginary parts of Z_in.
3. Compare against 4nec2/xnec2c and other references for trend consistency.

## Acceptance criteria

- No regression in existing parser/geometry/solver tests.
- Feedpoint-impedance trend becomes monotonic/stable with segmentation refinement.
- Documentation includes final equation conventions and units used in implementation.

## Implementation notes

- Keep CLI output contract unchanged.
- Introduce this as an internal solver path first, behind a clearly isolated API.
- Preserve room for later sinusoidal basis or higher-order basis options.

## KaTeX Formula Equivalents

$$
I_{\mathrm{seg}} = T a
$$

$$
Z T a = v
$$

$$
Z_{\mathrm{in}} = \frac{V_{\mathrm{source}}}{I_{\mathrm{source}}}
$$

$$
I_{\mathrm{tip}} = 0
$$

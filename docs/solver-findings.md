---
project: fnec-rust
doc: docs/solver-findings.md
status: living
last_updated: 2026-04-27
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

### EX type 3 source handling — REGRESSION-COVERED

As of 2026-04-27, EX type 3 support is locked at three layers while normalization
semantics remain intentionally deferred:

- Solver unit regression in `crates/nec_solver/src/excitation.rs`:
	`ex_type3_matches_ex_type0_vector`
- CLI integration regression in `apps/nec-cli/tests/ex_cards.rs`:
	`ex_type3_matches_ex_type0_feedpoint_impedance`
- Corpus regression deck and reference case:
	`corpus/dipole-ex3-freesp-51seg.nec` / `dipole-ex3-freesp-51seg`

Current behavior is explicit and test-locked: EX type 3 is accepted and produces
the same electrical excitation result as EX type 0 for equivalent card inputs.
Future changes to NEC normalization semantics should update these locks together.

On 2026-04-27, a non-breaking solver scaffold for EX type 3 normalization was
introduced in `nec_solver::excitation` via `Ex3NormalizationMode` and
`build_excitation_with_options(...)`. The production path still uses
`LegacyTreatAsType0`. As of 2026-04-28, CLI runtime wiring is available via
`--ex3-i4-mode <legacy|divide-by-i4>`, and Hallen RHS uses the same mode so
EX type 3 source normalization is consistent across solver paths.

### EX type 1 staged portability handling — REGRESSION-COVERED

As of 2026-04-28, EX type 1 is accepted on the same solver path currently used for
EX type 0 so portable decks no longer fail fast on this source family.

Current behavior is explicit and test-locked: EX type 1 is accepted, emits a CLI
warning that current-source semantics are still pending, and presently produces the
same excitation vector and Hallen feed behavior as EX type 0.

This is intentionally a compatibility bridge, not a physical implementation of NEC
current-source semantics.

Regression coverage exists at three layers:
- solver unit tests in `crates/nec_solver/src/excitation.rs`
- CLI warning/parity tests in `apps/nec-cli/tests/parser_warnings.rs` and `apps/nec-cli/tests/ex_cards.rs`
- corpus portability coverage via `dipole-ex1-freesp-51seg`

When true EX type 1 semantics are implemented, update all three layers together.

### EX type 2 staged portability handling — REGRESSION-COVERED

As of 2026-04-28, EX type 2 is accepted on the same solver path currently used for
EX type 0 so portable decks no longer fail fast on this source family.

Current behavior is explicit and test-locked: EX type 2 is accepted, emits a CLI
warning that incident-plane-wave semantics are still pending, and presently
produces the same excitation vector and Hallen feed behavior as EX type 0.

This is intentionally a compatibility bridge, not a physical implementation of
NEC incident-plane-wave excitation semantics.

Regression coverage exists at three layers:
- solver unit tests in `crates/nec_solver/src/excitation.rs`
- CLI warning/parity tests in `apps/nec-cli/tests/parser_warnings.rs` and `apps/nec-cli/tests/ex_cards.rs`
- corpus portability coverage via `dipole-ex2-freesp-51seg`

When true EX type 2 semantics are implemented, update all three layers together.

### EX type 4 staged portability handling — REGRESSION-COVERED

As of 2026-04-28, EX type 4 is accepted on the same solver path currently used for
EX type 0 so portable decks no longer fail fast on this source family.

Current behavior is explicit and test-locked: EX type 4 is accepted, emits a CLI
warning that segment-current semantics are still pending, and presently
produces the same excitation vector and Hallen feed behavior as EX type 0.

This is intentionally a compatibility bridge, not a physical implementation of
NEC segment-current excitation semantics.

Regression coverage exists at three layers:
- solver unit tests in `crates/nec_solver/src/excitation.rs`
- CLI warning/parity tests in `apps/nec-cli/tests/parser_warnings.rs` and `apps/nec-cli/tests/ex_cards.rs`
- corpus portability coverage via `dipole-ex4-freesp-51seg`

When true EX type 4 semantics are implemented, update all three layers together.

### EX type 5 staged portability handling — REGRESSION-COVERED

As of 2026-04-28, EX type 5 is accepted on the same solver path currently used for
EX type 0 so portable decks no longer fail fast on this source family.

Current behavior is explicit and test-locked: EX type 5 is accepted, emits a CLI
warning that qdsrc semantics are still pending, and presently
produces the same excitation vector and Hallen feed behavior as EX type 0.

This is intentionally a compatibility bridge, not a physical implementation of
NEC qdsrc excitation semantics.

Regression coverage exists at three layers:
- solver unit tests in `crates/nec_solver/src/excitation.rs`
- CLI warning/parity tests in `apps/nec-cli/tests/parser_warnings.rs` and `apps/nec-cli/tests/ex_cards.rs`
- corpus portability coverage via `dipole-ex5-freesp-51seg`

When true EX type 5 semantics are implemented, update all three layers together.

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

### Non-collinear loaded element case — BLOCKED ON SOLVER BREADTH

For `corpus/dipole-loaded.nec`, the current status is now explicit and tested:

- Hallen mode fails fast by design because the geometry is not collinear with the driven wire.
- Pulse, continuity, and sinusoidal modes all currently collapse to the same pulse-basis result on this topology.
- That result is not close enough to use as a parity substitute.

Observed fnec result at 7.1 MHz:

| Mode | Value |
|:-----|:------|
| pulse | **-13.7780 + j374.425 \u03a9** |
| continuity | **-13.7780 + j374.425 \u03a9** (fallback to pulse) |
| sinusoidal | **-13.7780 + j374.425 \u03a9** (fallback to pulse) |

External candidate currently tracked in the corpus:

| Reference | Value |
|:----------|:------|
| NEC2DXS500 via Wine | **13.4632 - j896.032 \u03a9** |

This is a sign and magnitude mismatch in both $R$ and $X$, not a small calibration delta.
The loaded-element corpus gap is therefore blocked by non-collinear solver support,
not by LD-card parsing or matrix-load application.

### Experimental non-collinear Hallen path (`--allow-noncollinear-hallen` flag)

As of 2026-04-25, an experimental opt-in path exists to allow non-collinear topologies
in the Hallen solver via feed-axis RHS projection. Results summary:

| Mode | Value | dX from external | Improvement vs pulse |
|:-----|:------|:-----------------|:---------------------|
| pulse baseline | **-13.7780 + j374.425 Ω** | 1270.46 Ω | baseline |
| hallen + `--allow-noncollinear-hallen` | **-45.0682 - j1008.139 Ω** | 112.11 Ω | **11.3× better** |
| external (NEC2DXS500) | **13.4632 - j896.032 Ω** | 0.0 Ω | — |

The experimental Hallen path achieves **11× improvement in reactance error** vs pulse 
but does not reach parity. Both real and imaginary parts remain far from the reference.

**Architectural limitation**: The improvement plateaus because the core issue is not
the RHS formulation but the matrix structure itself. All thin-wire MoM methods (Hallen,
Pocklington) weight off-diagonal matrix elements by $\cos(\alpha) = \mathbf{\hat{d}}_m \cdot \mathbf{\hat{d}}_n$ 
(the dot product of segment directions). For non-collinear geometries like the top-hat loop:

- For non-aligned segments, $|\cos(\alpha)| \approx 0$, making the matrix entries tiny
- This suppresses coupling between the main antenna and the loading loop
- The matrix becomes ill-conditioned and cannot recover the full 3D electromagnetic interaction

Tested improvements that did NOT help (2026-04-25):
- Blending perpendicular distance into the RHS (α = 0.5 weighting)
- Using pure Euclidean distance from feed point for non-collinear segments (α = 1.0)
- Both approaches made no measurable change to the impedance results

**Conclusion**: Forcing non-collinear support into thin-wire MoM via RHS tweaks alone
is fundamentally limited. A proper solution would require either:
1. Matrix reformulation for non-collinear topologies (substantial research effort)
2. Hybrid solver using different bases for collinear vs non-collinear parts
3. Acceptance that geometric loads (non-collinear loops) require surface or mixed-element modeling

For Phase 1 scope, the experimental path is adequate as a fallback that improves upon
pulse-baseline for users who understand its limitations, but is **not** a path to parity.

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
- Treat non-collinear loaded-element parity as a solver-breadth problem first; routing Hallen failures into the current pulse path only hides the real blocker.

## External references

- xnec2c source: https://github.com/KJ7LNW/xnec2c (primary NEC2 C reference)
- Burke & Poggio, "NEC2 Theory of Operation", LLNL 1981
- M5AIQ NEC resources: https://www.qsl.net/m5aiq/nec.html


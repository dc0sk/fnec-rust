---
project: fnec-rust
doc: docs/card-support-matrix.md
status: living
last_updated: 2026-07-05
---

# NEC Card Support Matrix

This document is the canonical reference for supported, partial, and deferred NEC card behavior in fnec-rust.  It covers every card that appears in the corpus or is relevant to 4nec2 / NEC2 compatibility.

Legend: **Full** — complete implementation; **Partial** — accepted and produces useful output, but some semantics are deferred (see Notes column); **Deferred** — parsed for portability but ignored at runtime with an explicit warning; **N/A** — not applicable or not in scope.

## Geometry cards

| Card | Support | Notes |
|------|---------|-------|
| CM / CE | Full | Comment cards; preserved in parse, ignored at runtime |
| GW | Full | Wire-segment definition |
| GE | Full | Geometry end; `GE I1=1` infers PEC ground when no GN card is present |
| GM | Full | Geometry move: rotate and/or translate wire ranges in place; `tag_increment > 0` appends transformed copies |
| GR | Full | Geometry repeat: successive z-axis rotation copies |
| GN type −1 | Full | Null-ground explicit free-space (same as omitting GN) |
| GN type 0 | Full | Simple finite ground (Sommerfeld / reflection-coefficient path) |
| GN type 1 | Full | Perfect-conductor (PEC) image method |
| GN type 2 | Partial | Low-height finite-conductivity near-ground path; scoped subset is regression-gated; other GN 2 geometries deferred |
| GN other | Deferred | Unsupported type: treated as free-space with a warning |

## Program-control cards

| Card | Support | Notes |
|------|---------|-------|
| FR | Full | Linear frequency sweep; all FR steps solved and reported |
| EN | Full | Terminates parse |

## Excitation cards

| Card | Support | Notes |
|------|---------|-------|
Excitation types follow canonical NEC2 numbering (see
`docs/ph8-chk-002-plane-wave-excitation.md`). Only type 0 is solved today; the
others are **recognized** (classified per NEC2 and given an accurate, category-named
diagnostic) but fail fast until their runtime semantics land in Phase 8 — they are
no longer silently treated as EX type 0.

| EX type 0 | Full | Applied-field voltage-gap source; supported across all solver paths (Hallen, pulse, continuity, sinusoidal) |
| EX type 1 | Partial | Incident plane wave, linear polarization. **Solves** on `--solver hallen` for a single straight wire (receiving antenna → induced `CURRENTS`, no feedpoint); validated vs nec2c shape + reciprocity (PH8-CHK-002). Straight non-junctioned multi-wire (parallel arrays) supported; **degree-2 junctioned geometry** (bends, start-to-start / end-to-end splits, inverted-V) now solves on continuous conductor paths (PH9-CHK-002 receive side, validated by reciprocity); degree-3+ (T/Y), closed loops, and `--solver pulse` fail fast. NTHETA×NPHI incidence-angle sweeps emit a `RECEIVE_PATTERN` (PH9-CHK-001) |
| EX type 2 | Partial | Incident plane wave, right-hand elliptic. **Solves** on `--solver hallen` for a single straight wire via the complex polarization vector (axial ratio F6); reduces to linear for a z-wire / AR=0; tilted-wire currents match nec2c (PH8-CHK-002). Non-junctioned multi-wire supported |
| EX type 3 | Partial | Incident plane wave, left-hand elliptic. Same as type 2 with opposite handedness. The legacy `--ex3-i4-mode` flag is an obsolete no-op |
| EX type 4 | Partial | Current source. **Solves** on `--solver hallen` for a single straight wire: forces the specified current and reports feedpoint `Z=V/i0` (equals the voltage-source impedance; PH8-CHK-001). Straight non-junctioned multi-wire (parallel arrays) supported; junctioned geometry and `--solver pulse` fail fast |
| EX type 5 | Partial | Voltage source (current-slope discontinuity). **Solves** as a voltage source via fnec's applied-field method — same result as type 0 (PH8-CHK-003). NEC's separate current-slope numerics (~6% different) are a documented non-goal |

## Load cards

| Card | Support | Notes |
|------|---------|-------|
| LD type 0 | Full | Series RLC: `Z = R + j(ωL − 1/(ωC))` |
| LD type 1 | Full | Parallel RLC: `Z = 1 / (1/R + 1/(jωL) + jωC)` |
| LD type 2 | Full | Series RL: `Z = R + jωL` |
| LD type 3 | Full | Series RC: `Z = R − j/(ωC)` |
| LD type 4 | Full | Series impedance (frequency-independent): `Z = R + jX` |
| LD type 5 | Full | Distributed wire conductivity: `Z = dl / (2π·a·σ)` |
| LD other | Deferred | Unknown type: load ignored with a warning |

### LD field mapping

`LD  type  tag  seg_first  seg_last  F1  F2  F3`

| Type | F1 | F2 | F3 |
|------|----|----|-----|
| 0 | R (Ω) | L (H) | C (F) |
| 1 | R (Ω) | L (H) | C (F) |
| 2 | R (Ω) | L (H) | — |
| 3 | R (Ω) | — | C (F) |
| 4 | R (Ω) | X (Ω) | — |
| 5 | σ (S/m) | — | — |

`tag=0` applies the load to all tags; `seg_first=0` applies to all segments of the tag; `seg_last=0` means same as `seg_first`.

## Network cards

| Card | Support | Notes |
|------|---------|-------|
| TL type 0 | Partial | Lossless; supported `NSEG` range: 0, 1, and >1 — all treated as a **single-section stamp** (no per-segment subdivision); `NSEG=0` is normalised to 1; stamps a 2-port admittance model into the Z matrix; `segment=0` maps to the tag center segment with a warning |
| TL other | Partial | Lossy line (`tl_type != 0`): stamps `Z0·coth/csch(γℓ)` with `F3` = matched-line loss in dB, velocity factor 1 (PH8-CHK-005). Reduces exactly to the lossless form at 0 dB |
| NT | Partial | Two-port network **stamped** into the Z matrix (`nec_solver::build_nt_stamps`, admittance→Z parameters `[Z]=[Y]⁻¹`; PH8-CHK-004). A well-formed reciprocal NT reproduces the equivalent TL feedpoint impedance end to end. Malformed / singular-admittance / missing-endpoint cards warn and are skipped |
| NE | Partial | Near electric field on a rectangular grid (PH9-CHK-004): Hertzian-element sum over the solved currents, emits a `NEAR_FIELD` section. Validated vs the far field at large range (0.02%). Spherical grids (`I1≠0`) and very-near-the-wire accuracy are out of scope |
| NH | Partial | Near magnetic field on a rectangular grid (PH9-CHK-004): azimuthal Hertzian-element sum over the solved currents, emits a `NEAR_H_FIELD` section. Validated by the far-field `|E|=η·|H|` relationship. Same scope limits as `NE` |
| PT | Partial | Print-control applied at runtime (PH9-CHK-004): `I1 ≤ −1` suppresses the current output, `I1 = 0` prints all, `I1 ≥ 1` restricts to tag `I2` / segment range `I3..I4`. Last PT card wins |

### TL field mapping

`TL  tag1  seg1  tag2  seg2  NSEG  type  F1  F2  F3`

| Field | Meaning |
|-------|---------|
| tag1, seg1 | Port 1 endpoint |
| tag2, seg2 | Port 2 endpoint |
| NSEG | Number of TL sections; supported range: 0, 1, or >1 — all use a single-section stamp (no subdivision); `NSEG=0` is normalised to 1 before stamping |
| type | 0 = lossless; non-zero = lossy (Z0·coth/csch(γℓ), F3 = matched-line loss dB) |
| F1 | Characteristic impedance Z₀ (Ω, default 50) |
| F2 | Transmission-line length (m) |
| F3 | Velocity factor (ratio, default 1.0) for lossless; angle (°) for lossy (deferred) |

## Output / post-processing cards

| Card | Support | Notes |
|------|---------|-------|
| RP | Full | Far-field radiation pattern; all RP grid points computed and included in the `RADIATION_PATTERN` report section. `XNDA` X-digit → `NORMALIZED_PATTERN` section (PH9-CHK-004) |

## Unknown / other cards

Any unrecognised card mnemonic is skipped with a parser warning emitted to stderr.  The solve proceeds on the remaining deck.

---

*See also*: [CLI guide](cli-guide.md) · [Design](design.md) · [Roadmap](roadmap.md)

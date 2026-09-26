---
project: fnec-rust
doc: docs/card-support-matrix.md
status: living
last_updated: 2026-09-26
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
| GM | Full | Geometry move, NEC-2 semantics: I2 = NRPT (number of new structures, 0 = move in place), F7 = ITS (the tag whose first segment begins the affected suffix, in definition order; 0 = whole structure). Copies are cumulative and tagged `tag + k*ITGI`; an in-place move applies ITGI to the tags too; tag-0 segments are never retagged; an ITS naming no wire is refused, as nec2c does. Trailing fields may be omitted. A negative ITGI is refused by name: it yields negative tag numbers that fnec's unsigned tags cannot represent. Pinned against nec2c in `crates/nec_solver/tests/gm_nec2c.rs` (captured 2026-08-30). One documented limit: a same-tag copy (ITGI=0) makes two segments share a (tag, index), and fnec addresses the first — nec2c can name the nth occurrence. No currently-working deck is affected; tracked as FND-135. Was wrong in two fields until FND-119 |
| GR | Full | Geometry repeat: successive z-axis rotation copies |
| GN type −1 | Full | Null-ground explicit free-space (same as omitting GN) |
| GN type 0 | Partial | Finite ground via a **normal-incidence scalar** reflection coefficient on the (correct-signed, PH9-CHK-006) image. Impedance is accurate (≈ Sommerfeld, ~10%, gated vs nec2c) for antenna heights ≥ ~0.2 λ; below 0.1 λ it is a reflection-coefficient approximation (no surface wave) and fnec **warns** — unless the exact **Sommerfeld surface wave** is enabled via `--ground-solver sommerfeld` for any straight wire — horizontal, vertical or tilted (PH9-CHK-006) — which reproduces nec2c GN2 incl. the low-height sign flip; bent or mixed geometry is declined with a warning and keeps the scalar-Γ result. Angle/polarization-dependent Fresnel (RCM) is deferred |
| GN type 1 | Full | Perfect-conductor (PEC) image method (correct-signed image, PH9-CHK-006) |
| GN type 2 | Partial | Aliases the GN0 scalar finite-ground path by default; the true Sommerfeld surface wave is available via `--ground-solver sommerfeld` for any straight wire, horizontal / vertical / tilted (feedpoint Z only, nec2c GN2, PH9-CHK-006; bent geometry is declined with a warning and keeps the scalar-Γ result), or via `--solver mpie` for the full near-ground **currents/patterns** on **any wire above ground** — horizontal / vertical / tilted straight wires (PH9-CHK-007 Phase D+E, reproduces nec2c GN2 to <8%, e.g. a vertical λ/2 dipole 84.5+j36 vs nec2c 89.75+j38.52) **and bent geometry** (per-segment-pair reflected reaction; an inverted-V over GN2 captures the surface-wave shift, R within ~9% of nec2c). Only a wire that reaches the `z=0` plane is rejected on the MPIE path |
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

| EX type 0 | Full | Applied-field voltage-gap source; supported across all solver paths (Hallen, pulse, continuity, sinusoidal, mpie). `--solver mpie` (PH9-CHK-007) additionally solves **degree-3 (T/Y) junctions** and **closed loops** — which the Hallen path returns unphysical junction-fed impedance for — and gives near-ground currents over Sommerfeld ground; it feeds the graph node nearest the driven segment |
| EX type 1 | Partial | Incident plane wave, linear polarization. **Solves** on `--solver hallen` for a single straight wire (receiving antenna → induced `CURRENTS`, no feedpoint); validated vs nec2c shape + reciprocity (PH8-CHK-002). Straight non-junctioned multi-wire (parallel arrays) supported; **degree-2 junctioned geometry** (bends, start-to-start / end-to-end splits, inverted-V) now solves on continuous conductor paths (PH9-CHK-002 receive side, validated by reciprocity); degree-3+ (T/Y), closed loops, and `--solver pulse` fail fast. NTHETA×NPHI incidence-angle sweeps emit a `RECEIVE_PATTERN` (PH9-CHK-001) |
| EX type 2 | Partial | Incident plane wave, right-hand elliptic. **Solves** on `--solver hallen` for a single straight wire via the complex polarization vector (axial ratio F6); reduces to linear for a z-wire / AR=0; tilted-wire currents match nec2c (PH8-CHK-002). Non-junctioned multi-wire supported |
| EX type 3 | Partial | Incident plane wave, left-hand elliptic. Same as type 2 with opposite handedness. The legacy `--ex3-i4-mode` flag is an obsolete no-op |
| EX type 4 | Partial | Current source (fnec's NEC-4-flavoured *segment* current; NEC-2's own EX 4 is an elementary Hertzian source at a POINT in space, a different drive, so nec2c cannot arbitrate between the two here). Since FND-118 a current source is the unit-voltage delta-gap solve rescaled by `i0 / I_feed`, which is exact for a linear system — so `Z = V/i0` equals the voltage-source impedance BY CONSTRUCTION, on every geometry and over any ground, not merely to within a tolerance. Before that it had its own augmented solver and agreed only in free space (0.02%), diverging 6.5% over ground and 6.4% on a bent conductor. Straight and degree-2 junctioned geometry both supported; degree-3+ (T/Y), closed loops and `--solver pulse` fail fast, and two current sources are refused (FND-129). Anchored against the MPIE kernel over ground in `apps/nec-cli/tests/current_source_ground_anchor.rs`, since the two drives agreeing is now definitional and cannot serve as its own gate |
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
| TL | Partial | NEC-2 layout `TL I1 I2 I3 I4 F1 F2 F3 F4 F5 F6 [F7] [F8]` (see field table). Solved as a two-port network connected **across the port segments' gaps, in parallel** (NEC-2's model), not stamped into the matrix: Y11 = Y22 = coth(γℓ)/Z0, Y12 = −csch(γℓ)/Z0 with γℓ = αℓ + j·kℓ/vf; the F3–F6 shunt admittances add to Y11/Y22; negative Z0 = crossed line. Lossy via the F8 extension (total matched-line loss dB). Validated vs nec2c: `TL 1 26 2 26 50.0 0.1` across two λ/2 dipoles 1 m apart → fnec 84.05+j28.65, nec2c 84.826+j31.131. fnec's retired layout (`… NSEG TYPE Z0 LEN [VF]`) is **refused** at parse time with the NEC-2 rewrite in the message (FND-111, FND-123) |
| NT | Partial | Two-port network given directly as Y11 Y12 Y22 (`NT I1 I2 I3 I4 Y11r Y11i Y12r Y12i Y22r Y22i`), solved across the port gaps in parallel, exactly like TL (FND-123). Several networks at one port add; both ports on one segment act as a one-port Y11+Y22+2·Y12. A zero-admittance NT opens the wire at its ports (KCL forces zero current), as in NEC. Validated vs nec2c: a 2 m, 50 Ω line's Y between segments 20 and 32 of a dipole → fnec 63.35−j213.68, nec2c 64.51−j212.78 |
| NE | Partial | Near electric field (PH9-CHK-004): Hertzian-element sum over the solved currents, emits a `NEAR_FIELD` section. Both **rectangular (`I1=0`) and spherical (`I1=1`)** grids are supported — spherical reinterprets the fields as `NX→R, NY→φ, NZ→θ` and maps to Cartesian, with point locations matching nec2c. Validated vs the far field at large range (0.02%); very-near-the-wire accuracy is out of scope |
| NH | Partial | Near magnetic field (PH9-CHK-004): azimuthal Hertzian-element sum over the solved currents, emits a `NEAR_H_FIELD` section. Rectangular and spherical grids supported (same convention as `NE`). Validated by the far-field `|E|=η·|H|` relationship |
| PT | Partial | Print-control applied at runtime (PH9-CHK-004): `I1 ≤ −1` suppresses the current output, `I1 = 0` prints all, `I1 ≥ 1` restricts to tag `I2` / segment range `I3..I4`. Last PT card wins |

### TL field mapping

`TL  I1  I2  I3  I4  F1  F2  F3  F4  F5  F6  [F7]  [F8]` (NEC-2 layout)

| Field | Meaning |
|-------|---------|
| I1, I2 | Tag and segment of end 1. Segment 0 is fnec shorthand for the tag's centre segment (the lower centre for even segment counts; a note is emitted) |
| I3, I4 | Tag and segment of end 2 (same segment-0 rule) |
| F1 | Characteristic impedance Z₀ (Ω). **Negative = crossed line** (180° reversal; negates Y12) |
| F2 | Line length (m). Zero or negative = the straight-line distance between the two segment centres (NEC-2 semantics) |
| F3, F4 | Shunt admittance at end 1, real and imaginary (S) |
| F5, F6 | Shunt admittance at end 2, real and imaginary (S) |
| F7 | fnec extension: velocity factor (default 1) |
| F8 | fnec extension: total matched-line loss in dB (default 0 = lossless) |

Missing trailing fields default to 0 as in NEC, except F7, which defaults to 1. `NSEG` and `TYPE` no longer exist.

**Retired layout.** fnec's old `TL t1 s1 t2 s2 NSEG TYPE Z0 LEN [VF]` is refused at parse time, and the error gives the NEC-2 rewrite — e.g. `TL 1 26 2 26 1 0 50.0 0.1 1.0` → `TL 1 26 2 26 50.0 0.1`; a lossy `TYPE=1` card (F3 = loss dB) → `… 0 0 0 0 1 <loss>`; a non-1 velocity factor → `… 0 0 0 0 <vf>`. Detection: ≥ 8 fields, field 5 a bare integer ≤ 16 and field 6 exactly `0` or `1`. A genuine NEC-2 card that happens to match is refused too; the message says to write Z₀ with a decimal point (e.g. `50.0`).

### TL / NT refusals

These are **errors, not warnings** (previously an unusable card was warned about and skipped, which solved a different antenna):

- any unusable TL/NT card: missing segment, fewer than 10 NT fields, non-numeric field, Z₀ = 0, velocity factor ≤ 0, loss < 0, electrical length a multiple of λ/2;
- a network with a non-Hallén solver (pulse / continuity / sinusoidal): "supported on --solver hallen only" (MPIE already rejected TL/NT);
- a network with a plane-wave or current-source drive: voltage (delta-gap) sources only.

The GPU-resident path declines any deck with TL/NT; such decks solve on the CPU.

## Output / post-processing cards

| Card | Support | Notes |
|------|---------|-------|
| RP | Full | Far-field radiation pattern; all RP grid points computed and included in the `RADIATION_PATTERN` report section. `XNDA` X-digit → `NORMALIZED_PATTERN` section; `XNDA` A-digit → `AVERAGE_POWER_GAIN` line (solid-angle-weighted mean gain = radiation efficiency over the full sphere, matches nec2c to <1%) (PH9-CHK-004). The `N`/`D` digits (gain-component labeling / dB-vs-ratio output-format toggles) are deferred — fnec always emits vertical/horizontal/total gain in dBi |

## Unknown / other cards

Any unrecognised card mnemonic is skipped with a parser warning emitted to stderr.  The solve proceeds on the remaining deck.

---

*See also*: [CLI guide](cli-guide.md) · [Design](design.md) · [Roadmap](roadmap.md)

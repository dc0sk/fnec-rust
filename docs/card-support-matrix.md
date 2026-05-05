---
project: fnec-rust
doc: docs/card-support-matrix.md
status: living
last_updated: 2026-05-05
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
| EX type 0 | Full | Voltage-gap source; supported across all solver paths (Hallen, pulse, continuity, sinusoidal) |
| EX type 1 | Partial | Implemented for `--solver pulse`; all other solver paths use staged portability (EX 0 behavior + warning) pending current-source semantics |
| EX type 2 | Partial | Staged portability; treated like EX type 0 with a warning; incident-plane-wave semantics pending |
| EX type 3 | Partial | `legacy` mode (default): treated like EX type 0 + non-default I4 warning; `--ex3-i4-mode divide-by-i4` enables experimental I4-divisor runtime semantics |
| EX type 4 | Partial | Staged portability; treated like EX type 0 with a warning; segment-current semantics pending |
| EX type 5 | Partial | Staged portability; treated like EX type 0 with a warning; electric-Hertz-dipole (qdsrc) semantics pending |

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
| TL type 0 | Partial | Lossless, `NSEG=0/1`; stamps a 2-port admittance model into the Z matrix; `segment=0` maps to the tag center segment with a warning |
| TL other | Deferred | Lossy / complex variants: card is ignored with a warning |
| NT | Deferred | Parsed for staged portability; ignored at runtime with a warning |
| PT | Deferred | Parsed for staged portability; ignored at runtime with a warning |

### TL field mapping

`TL  tag1  seg1  tag2  seg2  NSEG  type  F1  F2  F3`

| Field | Meaning |
|-------|---------|
| tag1, seg1 | Port 1 endpoint |
| tag2, seg2 | Port 2 endpoint |
| NSEG | Number of TL sections (0 or 1 = single section supported) |
| type | 0 = lossless (supported); non-zero = lossy/complex (deferred) |
| F1 | Characteristic impedance Z₀ (Ω, default 50) |
| F2 | Transmission-line length (m) |
| F3 | Velocity factor (ratio, default 1.0) for lossless; angle (°) for lossy (deferred) |

## Output / post-processing cards

| Card | Support | Notes |
|------|---------|-------|
| RP | Full | Far-field radiation pattern; all RP grid points computed and included in the `RADIATION_PATTERN` report section |

## Unknown / other cards

Any unrecognised card mnemonic is skipped with a parser warning emitted to stderr.  The solve proceeds on the remaining deck.

---

*See also*: [CLI guide](cli-guide.md) · [Design](design.md) · [Roadmap](roadmap.md)

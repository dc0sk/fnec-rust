---
project: fnec-rust
doc: docs/ph8-chk-002-plane-wave-excitation.md
status: living
last_updated: 2026-06-27
---

# PH8-CHK-002: incident plane-wave excitation (+ NEC2 EX-type alignment)

## Requirement / change

Roadmap `PH8-CHK-002`: implement incident plane-wave excitation as a real
receiving-antenna RHS — build the excitation from the incident field
(θ/φ direction, polarization, E-field magnitude) instead of a delta-gap, and
report induced segment currents + the open-circuit feedpoint voltage. Validate
against an external NEC2 reference.

## Decision 1 (user-approved 2026-06-27): align EX-type numbering to NEC2

Investigation found fnec's EX-type numbering is **non-standard** relative to NEC2,
verified with the installed `nec2c`:

| EX type | Canonical NEC2 (nec2c) | fnec today (incorrect) |
|:-------:|:-----------------------|:-----------------------|
| 0 | voltage source | voltage source ✓ |
| 1 | **incident plane wave, linear** | current source |
| 2 | incident plane wave, right-elliptic | incident plane wave |
| 3 | incident plane wave, left-elliptic | normalized voltage source |
| 4 | **current source** | segment current |
| 5 | voltage source (current-slope) | qdsrc |

`nec2c` accepts `EX 1 NTHETA NPHI 0 THETA PHI ETA` as a plane wave and produces no
feedpoint impedance (a scattering/receiving problem, not a driven source). Leaving
fnec's numbering as-is would make it **misread real 4nec2 plane-wave decks**
(which use `EX 1`) as current sources — directly defeating the deck-portability
goal of Phase 8. **Decision: align to NEC2.** Plane wave goes on type 1 (linear),
2 (right-elliptic), 3 (left-elliptic); the current source moves to type 4.

This is a breaking change to the existing *staged-portability* behaviour of EX
types 1/3/4/5 (currently routed to a "pulse current source" or warned). It is
staged: each renumbered type keeps a clear warning/fail-fast contract for the
sub-cases not yet solved.

## Decision 2: plane-wave RHS lives in the integral-equation forcing term

The current `build_hallen_rhs` is built specifically for the **delta-gap voltage
source**: its particular solution is the closed form `b_m = -j·(2π/η₀)·V·sin(k|s|)`.
A plane wave is a **distributed** incident field, so its forcing term is different.

For a straight wire along unit vector `û`, an incident plane wave
`E(r) = ê·E₀·exp(-jk·k̂·r)` has tangential field
`E_t(s) = (ê·û)·E₀·exp(-jk_s·s)` with `k_s = k(k̂·û)` — an exponential along the
wire axis. The Hallén/Pocklington particular solution for an `exp(-jk_s s)`
forcing is itself closed-form (`∝ exp(-jk_s s)/(k² − k_s²)` plus boundary terms),
so the straight-wire reference dipole is tractable. The implementation builds the
tangential incident field per segment and forms the matching forcing term, kept
consistent with the project's existing Hallén normalization.

NEC2 incidence convention (matched to `nec2c`): the wave arrives **from**
direction (θ, φ); `k̂ = -r̂(θ,φ)`. Polarization angle η orients **E** in the
(θ̂, φ̂) plane (η=0 → E along θ̂). E₀ defaults to 1 V/m.

## Validation strategy

1. **External NEC2 parity (`nec2c`)** — the harness deck
   `docs/dev/ph8-planewave-ref-theta30.nec` (`EX 1 1 1 0 30 0 0` + `XQ`) makes
   `nec2c` solve and print the induced `CURRENTS AND LOCATION` table; fnec's
   induced currents must match within the corpus tolerance. (nec2c needs an `XQ`
   execute card to print currents for a source-free scattering deck.)
2. **Reciprocity (internal, self-contained)** — by Rayleigh–Carson reciprocity the
   open-circuit receiving voltage of an antenna for a plane wave incident from
   (θ,φ) is proportional to its transmitting far-field at (θ,φ). This cross-checks
   the plane-wave solve against the already-validated RP far-field path without an
   external reference, across several angles.

## Staged delivery

1. **Foundation (this PR)** — NEC2 EX-type alignment in docs + the parser/model
   support for the plane-wave field layout (`ExCard` gains the polarization field;
   the parser reads it). The `nec2c` reference deck is checked in. No solve yet —
   plane-wave types are recognized **as plane waves** (NEC2-correct) on a clearly
   warned "pending" path, which already fixes the portability *misread*.
2. **Solve** — straight-wire EX type 1 linear-polarization plane-wave RHS on the
   Hallén path; induced-current report; nec2c + reciprocity gates.
3. **Breadth** — elliptic polarization (types 2/3); multi-angle NTHETA/NPHI sweeps;
   non-straight geometry.

## Test results

Recorded per increment in this section, the roadmap row, and the PRs.

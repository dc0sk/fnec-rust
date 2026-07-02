---
project: fnec-rust
doc: docs/ph8-chk-002-plane-wave-excitation.md
status: living
last_updated: 2026-07-02
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

### Increment 1 — foundation code (2026-07-02)

NEC2 EX-type alignment landed in the model, parser, and solver diagnostics
(the design/decision foundation was PR #255; this is the code foundation):

- **Model** (`nec_model::card`): added the `ExcitationKind` classifier (single
  source of the NEC2 0–5 numbering) and `ExCard::kind()`; added
  `ExCard::polarization_deg` (F3, the plane-wave polarization angle η).
- **Parser** (`nec_parser`): reads the F3 field into `polarization_deg`
  (defaults 0.0 when absent, so all existing decks are unaffected).
- **Solver** (`nec_solver::excitation`): the `UnsupportedType` diagnostic now
  names the NEC2 category — e.g. *"EX: incident plane wave, linear polarization
  (type 1) … is not yet supported"* instead of a bare type number. The
  `is not yet supported` substring (a corpus/test contract) is preserved.
- **Dormant path** (`nec-cli::solve_session`): the staged current-source path is
  re-pointed from the old `1|4|5` set to NEC2 type 4 via `ExcitationKind`.

Runtime acceptance is unchanged — types 1–5 still fail fast (the plane-wave and
current-source *solves* are the later increments) — so no corpus reference
contract changed. Added `ex_plane_wave_polarization_f3_is_captured` (parser).

Results: `cargo build --workspace` clean; `cargo test --workspace` **540 passed,
0 failed** (was 539; +1 new parser test); `cargo clippy --workspace` clean.
Manual check: a `EX 1 1 1 0 30 0 45` deck now reports the linear-plane-wave
diagnostic.

### Increment 2 — solve core (2026-07-02)

The straight-wire plane-wave Hallén solve, validated. Isolated in `nec_solver`;
the shared delta-gap `solve_hallen` path is **untouched** (no corpus risk).

- **Incident field** (`nec_solver::planewave`): `IncidentPlaneWave` from an EX
  type 1/2/3 card (θ/φ/η), NEC2 convention `k̂ = −r̂(θ,φ)`,
  `ê = cos η·θ̂ + sin η·φ̂`.
- **Forcing RHS** (`build_planewave_hallen`): the tangential incident field is
  integrated with the same Hallén normalization as the delta-gap builder —
  `rhs(sₘ) = −j·(2π/η₀)·Σ_p E_t(s_p)·sin(k|sₘ − s_p|)·Δl_p` — plus both `cos`
  and `sin` homogeneous columns.
- **2-DOF solve** (`solve_hallen_planewave`): unlike the delta-gap path (one
  `cos` constant per wire, enough for a *symmetric* source), this carries **both**
  homogeneous constants per wire — the two DOF classical Hallén needs to satisfy
  `I = 0` at both endpoints for an *asymmetric* plane-wave current.

**Validation** (`crates/nec_solver/tests/planewave_nec2c.rs`), reference decks
`docs/dev/ph8-planewave-ref-*.nec`:

1. **nec2c shape parity** — induced-current *distribution* on the λ/2 51-seg wire
   (θ=30) matches the `nec2c` table to **4.3%** of peak.
2. **Broadside symmetry** — θ=90 uniform illumination gives a symmetric current
   to **5×10⁻¹³**.
3. **Rayleigh–Carson reciprocity** — the receive short-circuit center current
   tracks the *validated transmit far-field*: `|I_center(θ)|²/G_θ(θ)` is constant
   across θ ∈ {40,55,70,90}° to **0.0000** spread (identical to 5 sig figs). This
   is the rigorous internal gate, independent of any external reference.

**Absolute-parity note (important):** fnec's Hallén operator and `nec2c` differ
systematically — even the *driven* dipole on this geometry shows a constant
complex offset (`nec2c/fnec ≈ 1.6∠−34°` in current; fnec Z ≈ 57−107 Ω vs nec2c
67−35 Ω on a coarse 21-seg wire; and 74.2+13.9 Ω vs 79.3+46.2 Ω on the 51-seg
λ/2). fnec's corpus impedance gates are **regression** gates against fnec's own
golden values, not tight nec2c parity. The plane-wave solve inherits exactly this
operator; the offset is a *constant complex factor shared with the delta-gap
solve*, removed by peak-alignment before the shape comparison. Hence the
validation gates are shape-parity + reciprocity, not absolute nec2c current
magnitude.

Results: `cargo test --workspace` **543 passed** (was 540; +3 plane-wave tests),
0 failed; clippy clean.

### Increment 3 — CLI wiring (2026-07-02)

Plane-wave decks are now user-runnable end to end.

- **Routing** (`nec-cli::solve_session`): the Hallén path detects a plane-wave EX
  card (`deck_has_plane_wave`) and routes the supported class — single straight
  wire, linear polarization (type 1) — through `solve_plane_wave_hallen`
  (`build_planewave_hallen` + `solve_hallen_planewave`). Diag label
  `hallen-planewave`.
- **Report**: no `FEEDPOINTS` rows for a receive solve (`build_feedpoint_rows`
  skips plane-wave cards, whose tag/segment fields are NTHETA/NPHI); the induced
  currents appear in the existing `CURRENTS` section; the incidence shows in
  `SOURCES`.
- **Fail-fast contracts kept**: elliptic (types 2/3) → "only linear … is
  implemented"; multi-wire type 1 → "multi-wire plane-wave is not yet supported";
  plane wave under a non-Hallén solver → "requires --solver hallen". All retain
  the `is not yet supported` / actionable-error contract.
- **Delta-gap builders** (`build_excitation`, `build_hallen_rhs`) now treat
  plane waves as *not a delta-gap source* (contribute nothing) rather than
  erroring, so the shared solve path is reached; types 4/5 still error.

**Contract updates**: `ex_cards.rs` and `parser_warnings.rs` type-1 tests flipped
from "rejected" to the accept-path (solves + `CURRENTS`; pulse → requires-hallen).
Corpus: `dipole-ex1-freesp-51seg` now solves (the impedance-centric corpus
framework has no receive-only case, so it is skipped there and validated by the
`nec_solver` planewave tests + the `ex_cards` CLI test); the pulse variant's
error contract became "requires --solver hallen".

Results: `cargo test --workspace` **544 passed**, 0 failed; clippy clean. Manual
CLI check: `EX 1 1 1 0 30 0 0` on `--solver hallen` emits induced `CURRENTS`
(endpoint ≈ 0, ramping to center) with an empty `FEEDPOINTS` section.

### Increment 4 — elliptic polarization, types 2/3 (2026-07-02)

Right- and left-hand elliptic plane waves now solve.

- **Field** (`nec_solver::planewave`): `IncidentPlaneWave` gains `axial_ratio`
  (EX F6) and `sense` (+1 type 2, −1 type 3). `pol_hat` returns a **complex**
  vector `ê = û_maj + j·sense·AR·û_minor` (major axis tilted by η, minor axis 90°
  out of phase, scaled by the axial ratio). The tangential coupling `ê·û` is
  complex; the existing forcing/solve already carry `Complex64`.
- **Model/parser**: `ExCard` gains `polarization_ratio` (EX F6), read by the
  parser. The excitation type sets the handedness.
- **Routing** (`nec-cli`): the elliptic fail-fast is removed; types 2/3 route to
  the same plane-wave solve as type 1.

**Validation** (`planewave_nec2c.rs`):
1. **z-wire reduction** — on a z-oriented wire only θ̂ couples (φ̂·ẑ = 0), so an
   elliptic wave (any AR) induces the same current as linear (exact, <1e-9).
2. **AR = 0 reduction** — type 2 with axial ratio 0 equals type 1 (exact).
3. **tilted-wire nec2c shape** — on a wire where both θ̂ and φ̂ couple, the
   induced-current distribution matches nec2c's elliptic reference to **5.4%**
   (a coarse, non-resonant tilted geometry; the same fnec-vs-nec2c operator
   offset applies). This confirms the axial-ratio physics, distinct from linear.
   (This symmetric geometry can't distinguish handedness by the shape metric —
   type 2/3 give conjugate-related currents that align to the same shape — but
   both reductions and the elliptic-vs-linear difference are validated.)

**Contract flips**: `dipole-ex2` / `dipole-ex3` (and the obsolete `dipole-ex3-i4-*`
/ `--ex3-i4-mode` variants) now solve as plane waves; their corpus contracts and
the `ex_cards` / `parser_warnings` type-2/3 tests were flipped from "rejected" to
the accept-path. The `--ex3-i4-mode` flag is retained as a documented no-op.

Results: `cargo test --workspace` **553 passed**, 0 failed; clippy clean.
`docs/card-support-matrix.md` EX types 1/2/3 → **Partial** (single straight wire).

### Increment 5 — non-junctioned multi-wire (2026-07-02)

`build_planewave_hallen` now supports **one or more straight, non-junctioned
wires** (e.g. a parallel dipole array). Each wire carries its own Hallén
particular solution: the tangential field uses that wire's axis, the along-wire
coordinate is measured from that wire's midpoint, and the `sin(k|sₘ−s_p|)` kernel
sums only over segments on the **same** wire. `solve_hallen_planewave` already
carries per-wire cos/sin homogeneous constants + two endpoint constraints per
wire. Junctioned geometry is rejected (`PlaneWaveError::JunctionedGeometryNotSupported`)
because its continuity constraints are not modelled.

**Validation** (`planewave_nec2c.rs`, two parallel z-dipoles at 28 MHz):
1. **Per-wire nec2c shape** — each wire's induced-current distribution matches
   nec2c to ~10–11% (aligned on its **own** peak). The fnec-vs-nec2c operator
   offset is *per-wire* here (wire 1 ≈1.49∠−26°, wire 2 ≈0.94∠−24°) — the same
   operator difference as the single wire, now manifested through the mutual
   coupling — so a single global alignment does not apply.
2. **Symmetric-broadside symmetry** — a wave from (θ=90, φ=90) hits two parallel
   z-wires with identical phase, so the induced currents on the two wires are
   **equal to 5×10⁻¹¹** — a self-consistent confirmation of the multi-wire
   coupling, independent of any external reference.
3. **Junction rejection** — junctioned geometry fails fast.

Results: `cargo test --workspace` **557 passed**, 0 failed; clippy clean.

Still pending: multi-angle NTHETA/NPHI sweeps; junctioned multi-wire (needs
continuity constraints in the plane-wave solve).

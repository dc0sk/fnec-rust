---
project: fnec-rust
doc: docs/project-review-feedback.md
status: living
last_updated: 2026-04-29
---

# Project Review Feedback

**Reviewer**: Claude Sonnet 4.6 (AI assistant)  
**Date**: 2026-04-29  
**Scope**: Codebase, documentation, tests, and design documents as of v0.2.0 / branch `feat/phase2-report-table-parity-next`

---

## 1. Project Understanding

fnec-rust is a Rust-native reimplementation of the NEC (Numerical Electromagnetics Code) antenna solver,
targeting practical compatibility with 4nec2 while being modular, testable, and portable. It is an
active single-developer project (DC0SK) in late Phase 1 / early Phase 2.

### Domain

NEC is a Method of Moments (MoM) solver for thin-wire antenna modeling, originally written in FORTRAN
(Burke & Poggio, 1981) and still the de facto standard for amateur and professional antenna work. NEC
"deck files" describe antenna geometry and simulation parameters in a fixed-column text format. Tools like
4nec2, EZNEC, and xnec2c provide GUIs and automation on top of NEC engines.

### What the project does today

- Parses 4nec2 / NEC-2 deck files (GW, GM, GR, GE, GN, EX, FR, RP, EN, LD, TL, PT, NT cards) with
  staged portability handling for features not yet fully implemented.
- Solves feedpoint impedance using a Hallén MoM solver for collinear thin-wire antenna topologies, with
  ground model support (free space, PEC image, GN0/GN2 Fresnel approximation).
- Computes segment current distributions and RP radiation patterns.
- Emits a versioned contract-bound CLI text report (`FORMAT_VERSION 1`) with FEEDPOINTS, SOURCES, LOADS,
  CURRENTS, RADIATION_PATTERN, and SWEEP_POINTS sections.
- Provides FR frequency sweep execution with rayon-parallelized multi-frequency solves.
- Includes experimental pulse/continuity/sinusoidal Pocklington solvers with a safety fallback from
  sinusoidal to Hallen when residual quality is poor.
- Scaffolds GPU acceleration dispatch (`nec_accel` crate) with a runtime mode selector (cpu/hybrid/gpu),
  but actual GPU kernels are stubs.

### Workspace structure

The codebase is a Cargo workspace of six library crates (`nec_parser`, `nec_model`, `nec_solver`,
`nec_accel`, `nec_report`, `nec_project`) and three application crates (`nec-cli`, `nec-gui`,
`nec-tui`). The CLI is the only production-ready frontend; GUI and TUI are stubs.

### Documentation and process quality

Unusually strong for an early-stage project: YAML frontmatter on all docs, PR-based update flow,
pre-commit hooks (fmt + test), pre-push security audit, CI corpus validation, frontmatter validation, and
a benchmark comparison workflow. Requirements, architecture, design, roadmap, and backlog are all
maintained as living documents. The tolerance matrix is formal, versioned, and CI-enforced.

---

## 2. Improvement Suggestions

The observations below are organized by the four areas requested: requirements, research, testing, and
architecture/design.

### 2.1 Requirements

#### R-01: Open GAPs have no owners or resolution criteria precise enough to action

GAPs 003, 004, 006, 007, 009–014 are all listed with priority but no assigned owner, no target milestone
linked to a specific phase, and no "done when X is true" definition sharp enough to close the gap without
a new round of scoping. A gap without an owner and a closure test is a wish, not a requirement.

**Suggestion**: For each open high-priority gap, add three fields: owner (name or role), target milestone
(phase + quarter), and one concrete closure criterion. Example for GAP-003: "MVP ground model set is done
when `dipole-gn2-near-ground-51seg` passes CI tolerance gates AND `docs/nec4-support.md` reflects the GN
subset as PARTIAL with rationale for deferred Sommerfeld."

#### R-02: NFR-006 (usability parity) is aspirational, not testable

"Usability must be competitive with incumbent tools" and "result inspection and repeat-run iteration must
be measurably efficient" are the right goals but have no measurement method. There is no user-observable
acceptance criterion.

**Suggestion**: Define at least one concrete usability benchmark per Phase 3 milestone. Examples: "A
repeat-run edit-run-inspect loop on a frequency sweep must complete in under N seconds from the CLI" or
"Setting up a 5-point FR sweep from scratch in the GUI must require fewer clicks than 4nec2 for the same
operation." These bind usability parity to something auditable.

#### R-03: FR-004 (Markdown-based project import/export) has no status or implementation track

This is a functional requirement with no corpus fixture, no backlog item, and no progress note in any
document. The `nec_project` crate exists but its scope is unclear.

**Suggestion**: Either promote FR-004 into the Phase 2 or Phase 3 backlog with a PAR item and owner, or
explicitly defer it to Phase 4+ with a rationale. Leaving it as a hanging FR creates confusion about when
it is expected.

#### R-04: The tolerance matrix does not cover the sinusoidal-to-Hallen fallback threshold

The matrix defines acceptance thresholds for final numerical results but does not define what constitutes
an acceptable residual for the sinusoidal solver before the fallback to Hallen is triggered. The current
threshold (`SINUSOIDAL_REL_RESIDUAL_MAX = 1e-2`) is a magic constant in `apps/nec-cli/src/main.rs`.

**Suggestion**: Add a solver-mode residual budget row to the tolerance matrix (or to the applied-math
doc), specifying what relative residual is acceptable for each solver mode and what triggers automatic
fallback. This connects the constant to the numerical contract.

#### R-05: The loaded-element parity gap is a Phase 1 completion blocker with no remediation plan

The roadmap states: "Close the remaining Phase 1 corpus gaps (loaded element reference parity and broader
non-collinear support)" and documents a specific known gap (`-13.778 + j374.425 Ω` vs external candidate
`13.463 - j896.032 Ω`). Phase 1 is estimated to close in Q2 2026, but there is no documented plan for
how this gap will be closed — only that it exists.

**Suggestion**: Either (a) document the research plan for resolving the non-collinear Hallen limitation
(which basis function change would fix this?), or (b) formally reclassify this as a Phase 2 goal and
update the Phase 1 completion criteria accordingly. The current state creates ambiguity about whether
Phase 1 is complete.

---

### 2.2 Research

#### RS-01: The GN0/GN2 Fresnel approximation lacks a documented accuracy bound

The current finite-ground model uses "a complex Fresnel-style reflection factor from EPSE/SIG" (per the
backlog progress note). This is acknowledged as an approximation of the Sommerfeld integral. However, no
document states: what is the accuracy of this approximation as a function of height above ground, ground
conductivity, and frequency? Where does it fail?

**Suggestion**: Add a section to `docs/applied-math.md` (or a new `docs/ground-model-accuracy.md`)
describing the Fresnel approximation's validity domain, its known failure modes (very low height, high
frequency, high-conductivity ground), and the path toward Sommerfeld integration. This is needed before
claiming NEC-4 class ground accuracy.

#### RS-02: The `studies/` directory scripts are informal and undocumented

The three Python scripts in `studies/mom-kernel-accuracy/` (`feedpoint_measurement.py`,
`hallen_reference.py`, `pocklington_study.py`) appear to be exploratory research scripts with a README.
Their relationship to the current solver implementation, what conclusions were drawn, and whether they are
still relevant is unclear from the docs.

**Suggestion**: Either formalize these as cited design artifacts (add references in `docs/applied-math.md`
to specific findings) or archive them with a summary of conclusions. As-is, they exist without
traceability to the solver implementation decisions they informed.

#### RS-03: No documented justification for quadrature order selection

The matrix assembly in `crates/nec_solver/src/matrix.rs` uses 4-point Gauss-Legendre for self elements
(after singularity subtraction) and 8-point GL for off-diagonal elements. The comment in the source
explains the split, but there is no documented study of whether these orders are sufficient for the
target segment lengths and wire radii. This matters for the convergence accuracy claims.

**Suggestion**: Add a brief benchmark to `docs/applied-math.md` or `docs/solver-findings.md` showing
that increasing the quadrature order further does not change impedance results beyond the tolerance matrix
threshold for representative geometries. This converts an implicit assumption into a documented design
choice.

#### RS-04: No plan for when to transition from Hallén to a more general MoM formulation

The Hallén solver is limited to collinear topologies. Many practically important antenna classes (Yagi
driven elements with in-line directors but bent reflectors, L-antennas, T-junctions, helicals) require
non-collinear support. The experimental pulse/sinusoidal solvers are described as diverging for thin wires.

**Suggestion**: Add a research milestone to the Phase 2/3 planning that explicitly defines: "What
formulation will replace or complement Hallén for non-collinear topologies?" Options include NEC2's
sinusoidal-basis Pocklington, the extended thin-wire kernel, or a full Galerkin MoM formulation. The
choice has architectural consequences and should be decided before Phase 3 GUI work begins, since the GUI
interaction model will be shaped by what geometries are solvable.

---

### 2.3 Testing

#### T-01: The solver crates have very few unit tests; coverage depends almost entirely on CLI integration tests

The only test file directly inside a solver crate is `crates/nec_solver/tests/pulse_rhs_scaling.rs`. All
other regression coverage runs through the CLI binary via `apps/nec-cli/tests/`. This means a regression
in the solver can only be caught by running the full CLI. It also means the solver crates are not testable
as libraries without the CLI.

**Suggestion**: Add unit tests for the Hallén matrix assembly, the Green's function integrator, the
excitation builder, and the geometry builder directly in the solver crate. At a minimum, test: (a) the
diagonal self-impedance element with known analytical approximations; (b) matrix symmetry for a
lossless two-segment case; (c) the ground image contribution sign for PEC.

#### T-02: No property-based tests for physical invariants

Several physical properties of correct NEC solvers are expressible as property checks: impedance matrix
symmetry for reciprocal structures, current conservation at junctions, linearity of superposition for
multi-source excitation, and pattern normalization. These would catch a class of bugs that corpus
regression cannot catch because the corpus only checks known-good outputs.

**Suggestion**: Introduce `proptest` (or `quickcheck`) for at least one physical invariant. A good
starting point: for a randomly parameterized dipole (within valid length/radius ratios), the impedance
matrix `Z` must satisfy `Z[i,j] = Z[j,i]` within floating-point tolerance. This would have caught
asymmetric ground-image implementations.

#### T-03: Parser has no fuzz testing

The NEC deck parser must handle malformed, truncated, and edge-case real-world files. The current parser
tests are hand-crafted cases. There is no fuzz corpus and no libFuzzer/AFL integration.

**Suggestion**: Add a cargo-fuzz target for the parser. Even a few hours of fuzzing will surface
unexpected panics on malformed input. This is especially important for the "tolerant enough for
real-world 4nec2 decks" claim in COMP-001, since real-world decks include formatting quirks that no
hand-crafted test will predict.

#### T-04: The benchmark comparison workflow has no trending history

The `scripts/pi-benchmark-compare.sh` and `.github/workflows/benchmark-compare.yml` support point-to-
point comparison between two CSV baselines. However, there is no mechanism to retain a series of baselines
and track performance trends over time. The `tmp/` directory contains several timestamped CSV files but
no tooling to aggregate them.

**Suggestion**: Add a lightweight benchmark history store (a CSV appended on each run, or a simple JSON
series) and a script to plot/display the trend. This turns the benchmark workflow from a regression gate
into an observable performance record, which will matter when GPU acceleration work begins.

#### T-05: The loaded dipole parity gap is tracked but not blocked from CI passing

The backlog notes that `dipole-loaded` returns a significantly wrong impedance for the non-collinear
geometry. The corpus case exists with an `--allow-noncollinear-hallen` opt-in path, but the *expected*
answer and the *actual* answer differ by an order of magnitude. CI passes because the case is gated as
"experimental" rather than "correct."

**Suggestion**: Add a separate CI job or test annotation that tracks the *distance* between the
experimental result and the external candidate, so that any inadvertent improvement (or regression) in
the non-collinear path is immediately visible. As-is, a solver change could make the answer worse and CI
would still pass.

---

### 2.4 Architecture and Design

#### A-01: `apps/nec-cli/src/main.rs` is a god file

The CLI entry point currently contains: argument parsing, execution mode selection, solver dispatch (all
four solver modes), FR sweep orchestration (single and multi-frequency), report rendering orchestration,
benchmark timing, GPU/hybrid dispatch routing, and all the format/warn helpers. This makes the file very
long and the solver dispatch logic untestable without invoking the full CLI binary.

**Suggestion**: Extract at minimum: (a) a `cli_args` module for argument parsing and validation; (b) a
`solve_session` struct or function that takes a parsed deck + options and returns a structured result
(decoupling solver dispatch from CLI formatting); (c) the sweep orchestration into a separate function.
This would make the solver path testable as a library function and allow future GUI/TUI frontends to
reuse it without re-implementing the dispatch logic.

#### A-02: The `nec_project` crate is a stub without a defined scope

The crate exists in the workspace and is declared in `Cargo.toml`, but `crates/nec_project/src/lib.rs`
appears to contain little or no functionality. FR-004 (Markdown-based project import/export) and the
Phase 3 project-oriented workflow both depend on this crate.

**Suggestion**: Either fill in a minimal scope for `nec_project` (what it will own: project file format,
run history, result storage?) or remove it from the workspace until it is needed. A stub crate with no
scope creates false confidence in the architecture and makes workspace builds slower for no gain.

#### A-03: No unified error hierarchy for library consumers

The solver crate exposes several separate error/warning types: `GeometryError`, `SolveError`,
`ExcitationError`, `LoadWarning`, `TlWarning`. These are not connected by a common trait or enum. A
library consumer (the embedding use case targeted by FR-008 and COMP-012) has no way to handle all solver
errors in one match arm or display them uniformly.

**Suggestion**: Introduce a top-level `NecError` enum in `nec_solver` (or a new `nec_core` crate) that
wraps all current error types, implements `std::error::Error`, and provides `From<T>` for each specific
error. This is a small change with large impact for embedding ergonomics and is best done before the
public API surface grows further.

#### A-04: The GPU dispatch architecture is a scaffold with no concrete backend timeline

The `nec_accel` crate exists, the `dispatch_frequency_point` seam is wired, and `DispatchDecision::
RunOnGpu` is handled non-fatally via a CPU emulation stub. However, DEC-008 specifies FOSS-first with
AMD preference, NFR-001a specifies Raspberry Pi VideoCore VI/VII as explicit targets, and the roadmap
phases GPU work to Phase 5 (Q2 2027). There is a risk that the dispatch architecture crystallizes around
one GPU API family before the VideoCore targets are prototyped — they require Mesa's OpenCL or Vulkan
compute, not ROCm.

**Suggestion**: Before the GPU kernel architecture is locked, add a document (or section in
`docs/architecture.md`) that: (a) defines the OpenCL/ROCm/Vulkan compute target matrix including Pi
VideoCore; (b) specifies which operations are candidates for first offload (matrix assembly vs. far-field
computation); and (c) defines the minimum viable GPU kernel that would validate the dispatch seam on real
hardware. This prevents the scaffold from being designed for one GPU family while the stated target
hardware requires another.

#### A-05: Dialect isolation is specified but not yet implemented

The architecture doc specifies that xnec2c dialect parsing must be isolated behind a trait or enum, with
the 4nec2 dialect path independently testable. The requirements specify auto-detection and explicit
override. However, the current parser appears to have no dialect abstraction layer — there is only one
parser path.

**Suggestion**: Before adding any xnec2c-specific parsing behavior, implement the dialect trait/enum
boundary first (even if both branches call the same parser). This makes the isolation architectural rather
than aspirational. Adding it after xnec2c-specific code exists is harder.

#### A-06: No documented segment count or memory limits

The Hallén matrix is O(N²) in memory and O(N³) to solve (via direct methods). For N=51 (the reference
dipole) this is trivial. For N=500 or N=5000 (large Yagi arrays or helix antennas), this will become a
real constraint. There is no documentation of expected limits, no memory estimate per segment count, and
no warning in the CLI when a large matrix is about to be assembled.

**Suggestion**: Add a section to `docs/benchmarks.md` (or a new section in `docs/applied-math.md`) that
documents the O(N²) memory scaling, provides example sizes for representative problem classes, and defines
the threshold above which the CLI should emit a warning. This is especially relevant for the Raspberry Pi
targets where available RAM is limited.

#### A-07: The `nec_report` crate API is not documented as a public surface

The report contract (PAR-001 v1) is well-specified in `docs/requirements.md`, and the crate exists, but
there is no documentation of what the `nec_report` public API looks like for consumers other than the
CLI. If the GUI or embedding consumers will use this crate directly, they need a documented interface.

**Suggestion**: Add a module-level doc comment in `crates/nec_report/src/lib.rs` describing: what input
types the crate accepts (`ReportInput`), what output it produces, and whether the rendered text is the
only interface or if structured data is also available. This is small work that will prevent the crate
from being re-implemented ad-hoc in each frontend.

---

## Summary Table

| ID | Area | Priority | One-line summary |
|:---|:-----|:--------:|:-----------------|
| R-01 | Requirements | High | Open GAPs need owners, milestones, and closure tests |
| R-02 | Requirements | Medium | NFR-006 usability goal needs a measurable benchmark |
| R-03 | Requirements | Medium | FR-004 Markdown project I/O has no implementation track |
| R-04 | Requirements | Medium | Sinusoidal fallback threshold is not part of the tolerance contract |
| R-05 | Requirements | High | Loaded-element parity gap has no remediation plan for Phase 1 close |
| RS-01 | Research | High | Fresnel ground approximation needs documented accuracy bounds |
| RS-02 | Research | Low | `studies/` scripts are informal and untraceable to design decisions |
| RS-03 | Research | Medium | Quadrature order selection lacks a documented convergence study |
| RS-04 | Research | High | No research plan for non-collinear MoM solver to replace/extend Hallen |
| T-01 | Testing | High | Solver crates need direct unit tests, not just CLI integration tests |
| T-02 | Testing | Medium | Physical invariants (matrix symmetry, superposition) should be property-tested |
| T-03 | Testing | Medium | Parser should be fuzz-tested against malformed real-world decks |
| T-04 | Testing | Low | Benchmark history has no trending or aggregation mechanism |
| T-05 | Testing | Medium | Loaded dipole parity gap is not CI-observable as a delta |
| A-01 | Architecture | High | `apps/nec-cli/src/main.rs` needs decomposition before further feature growth |
| A-02 | Architecture | Medium | `nec_project` crate is a stub with undefined scope |
| A-03 | Architecture | High | No unified error hierarchy for library consumers (embedding use case) |
| A-04 | Architecture | High | GPU dispatch scaffold needs a concrete backend/target plan before Phase 5 |
| A-05 | Architecture | Medium | xnec2c dialect isolation is specified but not yet structurally enforced |
| A-06 | Architecture | Medium | No documented segment count or memory limits for the solver |
| A-07 | Architecture | Low | `nec_report` crate public API is undocumented for non-CLI consumers |

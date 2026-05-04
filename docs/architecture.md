---
project: fnec-rust
doc: docs/architecture.md
status: living
last_updated: 2026-05-04
---

# Architecture

## System goals

- Build a Rust-native NEC-compatible solver with incremental NEC-2 and NEC-4 support.
- Provide a reusable core with separate CLI, GUI, and optional TUI frontends.
- Prioritize fast progress with simple ground handling first.
- Preserve room for extension through plugin/scripting capabilities.
- Reach explicit parity targets across three fronts: NEC-family numerical trust, 4nec2/EZNEC workflow usability, AutoEZ/xnec2c-optimize-class automation capability, and open-source embeddability/scriptability.

## Core architecture

1. Parse NEC deck input into an AST and diagnostics model.
2. Lower into validated domain model.
3. Build segmentation and physics model inputs.
4. Assemble and solve the numerical system on CPU.
5. Postprocess results (impedance, currents, radiation patterns).
6. Render user-facing 4nec2-like text reports.

## Frontend architecture

- CLI is the first production frontend and reference behavior.
- GUI on iced follows a modern, intuitive, task-oriented workflow.
- Optional TUI on ratatui shares core use cases.
- Frontends consume stable core APIs and must not embed solver logic.
- CLI behavior must remain strong enough to replace classic batch-oriented NEC tools for routine automation, not just human-operated one-off runs.

## Performance architecture

- CPU multithreading is baseline.
- Runtime acceleration mode selects CPU or GPU path when available.
- Initial GPU scope is postprocessing only; further offload is staged.
- Explicit portable-Linux hardware targets include Raspberry Pi 4 (VideoCore VI) and Raspberry Pi 5 (VideoCore VII), so acceleration design must keep ARM64 plus these GPU classes in scope.

## Extensibility architecture

- Plugin/scripting layer is in scope and planned as explicit extension points.
- Extension API must not break solver determinism guarantees by default.
- Core crates should remain embeddable enough to support future bindings and automation surfaces comparable to necpp-style library workflows.
- Automation architecture should be rich enough to support future variable-driven studies, resonance tools, and repeated-analysis workflows comparable in value to AutoEZ.

## Compatibility architecture constraints

- **4nec2 is the primary compatibility target** for both execution and workflow expectations.
- xnec2c input dialect is a secondary compatibility mode; it must not dilute or alter the 4nec2 primary standard.
- Text output format requires a stable contract before broad UI expansion.
- Numerical parity requires a tolerance matrix and reference corpus.
- Validation planning should explicitly map in-scope tests to NEC-5 Validation Manual scenario classes where equivalent model classes exist.
- EZNEC is a major workflow/usability comparator even though its engine model differs; fnec-rust should match or exceed it in common user tasks over time.
- AutoEZ is a major automation-workflow comparator; fnec-rust should eventually support comparable variable-study and repeated-analysis workflows without requiring spreadsheet coupling.
- xnec2c is the main open-source Linux/Unix workflow comparator; xnec2c-optimize is the external optimizer-loop comparator; yeti01/nec2 is the classic batch-CLI comparator; necpp is the embeddable automation comparator.

## Input dialect model

fnec-rust supports multiple NEC input dialects through a layered parser architecture:

| Dialect | Status | Notes |
|:--------|:------:|:------|
| 4nec2 | Primary | The canonical target; all real-world 4nec2 decks must parse correctly |
| xnec2c | Secondary | Where xnec2c input diverges from 4nec2, it is treated as a distinct dialect |

### Auto-detection

- The parser attempts automatic dialect detection before user-visible mode selection.
- Detection is heuristic and based on structural markers unique to each dialect.
- If detection is ambiguous, the parser defaults to 4nec2 mode and emits an informational diagnostic.
- Users may override dialect detection explicitly via a CLI flag or project frontmatter field.
- A detected dialect is recorded in the project model and surfaced in report headers.

### Implementation rule

- Dialect-specific parsing logic must be isolated behind a dialect trait or enum; it must not be mixed into the shared NEC2 core parser.
- The 4nec2 dialect path must remain independently testable with no xnec2c-specific code on its execution path.

## Reference engine: xnec2c

- xnec2c (KJ7LNW fork, https://github.com/KJ7LNW/xnec2c) is used as the authoritative NEC2 reference engine for:
  - Generating golden test corpus outputs against which fnec-rust numerical results are compared.
  - Algorithmic study of NEC2 numerical methods as implemented in C.
- **Policy constraint**: xnec2c is GPL-3.0-only. fnec-rust is GPL-3.0-only (license-compatible). Despite this, no code from xnec2c may be copied, translated, or adapted into fnec-rust source under any circumstances — this is a project independence policy, not a license barrier.
- xnec2c's `examples/` and `t/` directories are the primary source for the test corpus `.nec` input files.

## Additional reference implementations

The following projects are useful comparative references for algorithms, behavior checks, and implementation ideas:

- M5AIQ NEC notes/tools: https://www.qsl.net/m5aiq/nec.html
- yeti01/nec2: https://github.com/yeti01/nec2
- tmolteno/necpp: https://github.com/tmolteno/necpp
- KJ7LNW/xnec2c-optimize: https://github.com/KJ7LNW/xnec2c-optimize
- GNU NEC (SourceForge): https://sourceforge.net/projects/gnu-nec/
- NEC-5 Validation Manual (Burke, 2019): https://ipo.llnl.gov/sites/default/files/2020-07/NEC5%20Validation%20Manual%20092419.pdf

These references are supplementary, but not irrelevant. 4nec2 compatibility remains the primary product target, xnec2c remains the main NEC2 parity reference corpus source, xnec2c-optimize remains a key baseline for external optimizer-loop ergonomics, yeti01/nec2 and GNU NEC remain useful baselines for clean open NEC2 batch execution, necpp remains a useful baseline for library-oriented automation and geometry diagnostics, and the NEC-5 Validation Manual is used to structure higher-difficulty validation classes.

## Future extensions (Phase 5+)

These are not in scope for Phase 1–4 but are documented here as architectural commitments — the
core design must not make them gratuitously harder.

### Large-N MoM acceleration

The dense O(N²) impedance matrix assembly and O(N³) LU solve are practical up to approximately
N = 2 000 segments on a developer workstation and N = 500 on Raspberry Pi 4. Beyond those
thresholds, explicit matrix acceleration is required.

**Planned path**:

1. **Adaptive Cross Approximation (ACA)** — Phase 5, first priority.
   ACA compresses the assembled Z matrix algebraically: off-diagonal blocks between well-separated
   segment groups are replaced by low-rank factorizations U·V^T found via adaptive pivot sampling.
   The result is an H-matrix that supports O(N log N) matrix-vector products, enabling iterative
   solvers (GMRES) to replace direct LU. ACA is purely algebraic — no physics-specific code —
   and is straightforward to validate by comparing compressed and uncompressed solutions on the
   same corpus decks.
   - Integration point: `nec_solver` matrix assembly produces a dense Z; a new
     `nec_solver::compress` module (or `nec_accel` sub-path) applies H-matrix ACA compression
     before the solve step.
   - The existing dense path remains the default; ACA is opt-in via `--accel aca` or activated
     automatically above a segment-count threshold.

2. **Fast Multipole Method (FMM)** — Phase 5+, if N > 20 000 becomes a realistic need.
   FMM avoids assembling Z at all; matrix-vector products for GMRES are computed directly via
   multipole expansions of the Green's function. O(N) complexity asymptotically, but requires
   EM-specific multipole expansion code (vector spherical harmonics) and an effective
   preconditioner for GMRES convergence. Implementation cost is 5–10× that of ACA.

3. **Canning IML / Simply Sparse** — evaluate at Phase 5 alongside ACA; deprioritize if ACA
   delivers sufficient scaling, as IML is harder to verify and less widely validated.

Full analysis, scaling thresholds, and implementation guidance are in
[docs/utd-feasibility-assessment.md](utd-feasibility-assessment.md).

### Hybrid MoM-UTD

Hybrid MoM-UTD adds capability for antennas on or near electrically large platforms (finite
ground planes, vehicles, ships, buildings). The wire MoM solver does not change; instead, UTD
scattered-field contributions are added as corrections to the impedance matrix during assembly:

```
[Z_mm + Z_mm^{UTD}] I_m = V_m
```

Integration point: the impedance matrix assembly function (`assemble_z_matrix_with_ground` or a
new `assemble_z_matrix_with_utd_bodies`). The Hallén solver, parser, report contract, and corpus
framework are all unaffected when UTD bodies are absent.

Key dependencies not yet located: the NEC-BSC reference implementation (hybrid MoM-UTD platform
code by Burke / Ohio State ESL). Foundational theoretical references: Kouyoumjian & Pathak (1974)
for PEC wedge UTD coefficients; Lertwiriyaprapa, Pathak & Volakis (2007) for material-boundary
extensions.

Target phase: Phase 4–5 for finite-ground edge diffraction; Phase 5 for full antenna-on-platform.

### Time-domain / broadband conversion (TD-UTD)

Rousseau & Pathak (1996) — DTIC ADA305743 — extend frequency-domain UTD to transient/pulsed
excitation via the Analytic Time Transform (ATT). This is relevant to fnec-rust only if broadband
sweep → time-domain conversion is added (e.g., IFFT of complex impedance or radiation pattern
sweeps). The ATT provides a mathematically sound way to invert UTD field expressions into the
time domain without the causality artifacts of naive IFFT.

Target phase: Phase 5+ (contingent on broadband/transient use-case demand).

## Documentation process constraints

- Docs updates flow through PRs only due to protected main.
- Frontmatter validation and stamping automation remain required quality gates.

## Contributor orientation

New contributors should read [docs/contributing.md](contributing.md) for build instructions,
branch conventions, the pre-push sequence, and corpus-gate requirements before opening a PR.

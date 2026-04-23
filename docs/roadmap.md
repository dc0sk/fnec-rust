---
project: fnec-rust
doc: docs/roadmap.md
status: living
last_updated: 2026-04-23
---

# Roadmap

## Roadmap principles

- **Staged delivery**: Each phase builds on prior phases; no phase skips its blockers.
- **Tolerance-first testing**: Numerical results are measured against a curated reference corpus with explicit tolerance per metric (impedance, gain, pattern, current). No solver expansion without validated corpus pass.
- **Explicit scope boundaries**: NEC-4 support is versioned and partial; users must see what is and isn't supported. Ground models, source types, and frequency sweeps each have documented roadmaps.
- **FOSS and diversity prioritized**: GPU acceleration favours FOSS frameworks (OpenCL, SYCL, HIP) and AMD GPU support to avoid vendor lock-in.

## Phase 0 (done/in place)

- Documentation baseline established under docs/ with YAML frontmatter.
- PR-based last_updated automation path defined for protected main.
- Hallén MoM solver validated: 51-segment λ/2 dipole → 74.24 + j13.90 Ω (matches Python reference).
- Pulse/continuity solver modes marked EXPERIMENTAL (divergence root-caused).
- Core requirement, gap, tolerance matrix, and architectural docs in place.

**Blocker**: BLK-001 (tolerance matrix) — **resolved** 2026-04-22.

## Phase 1 (current focus): NEC foundation and fast progress

**Goals**: Solver breadth, text output contract, golden test corpus.

**Key deliverables**:
- ✅ Hallén solver working and validated (done).
- ✅ Golden reference corpus scaffolded: 6 benchmark geometries, reference results template, validation test (done, `feat/golden-corpus-validation` branch).
- ✅ NEC-4 feature boundary documented: card support matrix, phase assignments (done, `docs/nec4-support.md`).
- [ ] Expand NEC-2 card support breadth (FR, GN, GE, geometry edge cases).
- [ ] Build 4nec2-like text report format (output sections, units, precision).
- [ ] Assemble golden reference corpus (half-wave dipole free-space/over-ground, Yagi, loaded element, frequency sweep, multi-source).
- [ ] Run corpus through reference and fnec-rust; validate all results within tolerance matrix.
- [ ] Simple ground model (infinite, raised dielectric) working and tested.
- [ ] CLI-first execution flow complete with all core flags (solver mode, solver options).

**Blocker dependencies**: BLK-003 (4nec2 report format) and BLK-002 (NEC-4 feature boundary).

**Estimated completion**: Q2 2026 (end of April).

## Phase 2: Compatibility expansion and confidence

**Goals**: NEC-4 subset, golden corpus pass, production readiness.

**Key deliverables**:
- [ ] NEC-4 feature boundary clearly defined and documented (BLK-002).
- [ ] NEC-4 subset implemented (e.g., loads, sources, excitation types; exclude: near fields, extended patterns).
- [ ] Golden test corpus corpus passes all fnec-rust solvers within tolerance matrix (BLK-001).
- [ ] CI enforces corpus tolerance pass as a gate on every commit.
- [ ] Advanced ground models added (Sommerfeld, buried antennas).
- [ ] Multi-wire and complex geometry edge cases validated.
- [ ] Pulse/continuity modes fixed via sinusoidal-basis EFIE or remain experimental with clear warnings.

**Estimated completion**: Q3 2026 (end of July).

## Phase 3: UX and workflow productization

**Goals**: GUI, project workflows, user experience polish.

**Key deliverables**:
- [ ] Modern, intuitive task-oriented GUI on iced.
- [ ] GUI/CLI behavior parity for core tasks (deck parse, solve, report).
- [ ] Project-oriented workflows (import, export, run history, result storage).
- [ ] Plugin API design and safety model baseline (BLK-004).
- [ ] Comprehensive contributor and user documentation.

**Estimated completion**: Q4 2026 (end of October).

## Phase 4: Extensibility

**Goals**: Plugin system, scripting hooks, community extensions.

**Key deliverables**:
- [ ] Plugin/scripting architecture designed and documented.
- [ ] First stable extension points (deck post-processors, result filters, custom reports).
- [ ] Sandboxing model and dependency policy (BLK-005, GPLv2 compatibility).
- [ ] Plugin registry and distribution mechanism.

**Estimated completion**: Q1 2027 (end of March).

## Phase 5: Performance scaling

**Goals**: GPU acceleration from postprocessing to solver kernel.

**Key deliverables**:
- [ ] GPU acceleration for postprocessing (pattern interpolation, report generation).
- [ ] Benchmark CPU vs GPU behavior; define selection criteria.
- [ ] Plan staged expansion to matrix fill and solve on AMD ROCm / OpenCL / SYCL (DEC-008).
- [ ] Prototype GPU solver kernel on reference geometry.
- [ ] CI benchmarking dashboard for performance tracking.

**Estimated completion**: Q2 2027 (end of June).

## Gaps and blockers (from requirements.md)

| Gap | Title | Priority | Target | Owner | Resolution criteria |
|:---|:------|:---------|:-------|:------|:-------------------|
| GAP-002 | NEC-4 feature boundary | **CRITICAL** | Phase 2 end | (TBD) | BLK-002: Explicit list of supported/deferred NEC-4 cards/features in docs/nec4-support.md |
| GAP-003 | MVP ground model set | **HIGH** | Phase 1 end | (TBD) | Simple (infinite, raised dielectric) implemented; advanced (Sommerfeld, buried) in Phase 2 plan |
| GAP-004 | Plugin/scripting interface | **HIGH** | Phase 3 end | (TBD) | BLK-004: API design, safety model, first two extension points documented and working |
| GAP-005 | 4nec2-like text report format | **HIGH** | Phase 1 end | (TBD) | BLK-003: Sections, units, precision, ordering contract locked; corpus results validated |
| GAP-006 | GUI information architecture | **MEDIUM** | Phase 3 end | (TBD) | IA document, wireframes, and user testing feedback collected |
| GAP-007 | GPU rollout criteria | **MEDIUM** | Phase 5 end | (TBD) | Framework selection follows DEC-008 (FOSS-first, AMD-preferred); benchmarks published |
| GAP-008 | Dependency/license policy | **MEDIUM** | Phase 2 end | (TBD) | BLK-005: Policy thresholds, exception process, GPLv2 compatibility rules documented |

## Gap-driven milestone blockers (Phase gates)

| Blocker | Resolution condition | Gate |
|:--------|:---------------------|:-----|
| BLK-001 | Tolerance matrix defined with metrics for R, X, gain, pattern, current, phase | Phase 1 → Phase 2 |
| BLK-002 | NEC-4 feature boundary documented (supported/deferred cards/sources); see [docs/nec4-support.md](docs/nec4-support.md) | Phase 1 → Phase 2 |
| BLK-003 | 4nec2 text report format contract locked; golden corpus results validated within tolerance | Phase 1 → Phase 2 |
| BLK-004 | Plugin API design, safety model, first two extension points working | Phase 3 → Phase 4 |
| BLK-005 | GPLv2 dependency policy thresholds and exception process documented | Phase 2 → Phase 3 |

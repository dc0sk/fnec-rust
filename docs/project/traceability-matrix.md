---
project: fnec-rust
doc: docs/project/traceability-matrix.md
status: living
last_updated: 2026-07-02
---

# Traceability matrix

The end-to-end matrix tying **requirement → design → implementation → tests →
result** for every delivery unit. Two views:

- **View A — requirement coverage**: each top-level requirement family → the
  phase/checklist(s) that satisfy it → state.
- **View B — checklist delivery matrix**: each `PHx-CHK-*` item → its full
  five-layer chain.

Column legend for View B: **Req** = requirement/gap IDs; **Design** = the design
doc node; **Impl** = primary implementation modules; **Tests** = the gating test
artifact(s); **Result** = recorded outcome. Detail behind each column lives in the
sibling layer docs ([requirements](requirements-register.md) ·
[design](architecture-design-index.md) · [implementation](implementation-map.md) ·
[tests](test-catalog.md) · [results](test-results.md) ·
[tooling](tooling-catalog.md)).

Status legend: ✅ done · 🔨 in progress · 📋 planned.

---

## View A — requirement coverage

| Requirement | Satisfied by | State |
|:------------|:-------------|:------|
| FR-001 reusable Rust crates | 7 crates + 2 apps | ✅ |
| FR-002 CLI + GUI | `apps/nec-cli`, `apps/nec-gui` | ✅ |
| FR-003 execute real NEC decks | Phases 1–2; Phase 8 residual cards | 🔨 (Phase 8) |
| FR-004 Markdown project I/O | `nec_project` (GAP-015) | ✅ |
| FR-005 4nec2-like text reports | PH2-CHK-004 report contract v1 | ✅ |
| FR-006 plugin/scripting | EP-1..4 (Phase 3–4) | ✅ |
| FR-007 batch/sweep workflows | PH3-CHK-006/007/008 | ✅ |
| FR-008 stable automation APIs | PH4-CHK-003/004/006 | ✅ |
| FR-009 geometry diagnostics | PH2-CHK-006 | ✅ |
| FR-010 resonance/matching helpers | PH3-CHK-008 | ✅ |
| NFR-002 multithreaded/deterministic CPU | rayon solve paths | ✅ |
| NFR-003 optional GPU + CPU fallback | Phases 5–7 | ✅ |
| NFR-004 per-metric tolerance | corpus + `reference-results.json` | ✅ |
| NFR-005 script-friendly streams | PH2-CHK-008 | ✅ |
| NFR-006 competitive usability | PH3-CHK-012 | ✅ |
| COMP-001 tolerant parsing | `nec_parser` | ✅ |
| COMP-002/008 tolerance-gated accuracy | corpus gate | ✅ |
| COMP-003 versioned NEC-4 scope | `nec4-support.md`, `card-support-matrix.md` | ✅ (living) |
| PRT-001 ground modeling | Phase 2; PH8-CHK-006 extends | 🔨 (Phase 8) |
| PRT-002 loads/TL/network/source | Phase 2; PH8-CHK-001..005 | 🔨 (Phase 8) |
| PRT-003 sweep/gain/pattern/report | Phase 1–2 | ✅ |
| PRT-004 GUI/workflow | Phase 3 | ✅ |
| PRT-005/006 optimization/automation | Phase 3–4 | ✅ |
| PRT-007 geometry/embeddability | Phase 2–4 | ✅ |
| PRT-008 accuracy breadth | Phase 1–3 corpus | ✅ |
| PRT-009 NEC-5 surfaces | wire-only decision (`nec5-frontier.md`) | ✅ (decided) |
| PRT-010 NEC-5 validation matrix | PH2-CHK-007 | ✅ |
| PRT-011 distributed execution | Phases 6–7 | ✅ |
| CP-003 deck-portability cards | **Phase 8 (in flight)** | 🔨 |
| GAP-002..015, BLK-001..005 | see [register](requirements-register.md) | ✅ all resolved |

---

## View B — checklist delivery matrix

### Phase 1 — NEC foundation (complete, v0.3.0)

Delivered as roadmap key-deliverables rather than numbered CHK rows. Chain:
**Req** FR-003/005, PRT-003/008, DEC-010 → **Design** `applied-math.md`,
`architecture.md` → **Impl** `nec_parser`, `nec_solver` (geometry/matrix/linear),
`nec_report` → **Tests** `corpus_validation.rs`, `report_contract.rs`,
`topology_fallback.rs` → **Result** reference dipole 74.24+j13.90 Ω; corpus green.
✅

### Phase 2 — compatibility expansion (complete, v0.5.0)

| ID | Req | Design | Impl | Tests | Result | S |
|:---|:----|:-------|:-----|:------|:-------|:-:|
| PH2-CHK-001 | PRT-001, CP-002 | `nec4-support.md` | `nec_solver/geometry.rs` (GroundModel) | `ground_diagnostics.rs`, `corpus_validation.rs` | GN2 solves; 6 ground fixtures gated | ✅ |
| PH2-CHK-002 | PRT-001/008 | `nec4-support.md` | `nec-cli/geometry_validation.rs` | `ground_diagnostics.rs` | buried-wire fail-fast; near-ground gated | ✅ |
| PH2-CHK-003 | PRT-002, CP-003 | `card-support-matrix.md` | `nec_solver/loads.rs`, `tl.rs` | `ld_loads.rs`, `tl_cards.rs`, `parser_warnings.rs` | LD 0–5, TL lossless, NT portability | ✅ |
| PH2-CHK-004 | PRT-003, CP-001 | `design.md` | `nec_report/lib.rs` | `report_contract.rs`, `scriptability_contract.rs` | 6 stable sections, ordered | ✅ |
| PH2-CHK-005 | PRT-008, CP-001 | `corpus-validation-strategy.md` | `nec_solver/farfield.rs` | `corpus_validation.rs` | RP/gain/current gates added | ✅ |
| PH2-CHK-006 | PRT-007, CP-007 | `design.md` | `nec-cli/geometry_validation.rs` | `geometry_diagnostics.rs` | intersection/source/junction gates | ✅ |
| PH2-CHK-007 | PRT-010 | `corpus-validation-strategy.md` | corpus matrix rows | `corpus_validation.rs` | `PH2N5-001..010` traceable | ✅ |
| PH2-CHK-008 | PRT-003/007 | `design.md` | `nec-cli/main.rs`, `warnings.rs` | `scriptability_contract.rs`, `core_flags_contract.rs` | stream/exit-code contracts locked | ✅ |

### Phase 3 — UX & workflow (complete, v0.5.0)

| ID | Req | Design | Impl | Tests | Result | S |
|:---|:----|:-------|:-----|:------|:-------|:-:|
| PH3-CHK-001 | GAP-002 | `nec4-support.md` | card status index | `corpus_validation.rs` | 25-row card table | ✅ |
| PH3-CHK-002 | PRT-004, GAP-009 | `contributing.md` | docs | frontmatter gate | contributor guide | ✅ |
| PH3-CHK-003 | GAP-004, BLK-004 | `plugin-api-design.md` | `nec_model`, `nec_report` (EP-1/2) | doctests | EP-1/EP-2 exercised | ✅ |
| PH3-CHK-004/005 | PRT-004, GAP-010 | `project-format.md` | `nec_project/lib.rs` | `project_roundtrip.rs` | project + run history | ✅ |
| PH3-CHK-006 | PRT-005 | `automation-guide.md` | `nec-cli/sweep_config.rs` | `sweep_contract.rs` | `--sweep-config` | ✅ |
| PH3-CHK-007 | PRT-005/006 | — | `nec_parser/template.rs`, `nec-cli/vars_config.rs` | `template_contract.rs` | `--vars` engine | ✅ |
| PH3-CHK-008 | PRT-006, GAP-012 | — | `nec-cli/resonance_search.rs` | `resonance_contract.rs` | bisection resonance | ✅ |
| PH3-CHK-009..011 | PRT-004 | `design.md` | `nec-gui/*` | `gui_smoke.rs` (47) | solve/sweep/pattern/currents | ✅ |
| PH3-CHK-012 | GAP-009, PRT-004 | `usability-benchmark-ph3.md` | — | benchmark record | ≤7-action sweep; vs xnec2c | ✅ |

### Phase 4 — extensibility (complete, v0.5.0)

| ID | Req | Design | Impl | Tests | Result | S |
|:---|:----|:-------|:-----|:------|:-------|:-:|
| PH4-CHK-001 | GAP-008, BLK-005 | `dependency-policy.md` | `deny.toml`, SBOM | `cargo deny check licenses` | 13-ID allowlist; clean | ✅ |
| PH4-CHK-002 | GAP-004 | `plugin-api-design.md` | `nec_report` (EP-3) | doctests + unit | ReportSection | ✅ |
| PH4-CHK-003 | FR-008, PRT-007 | `json-output-schema.md` | `nec-cli` `--output-format json` | `json_output_contract.rs` | schema v1 stable | ✅ |
| PH4-CHK-004 | FR-008, COMP-012 | `python-bindings.md` | `bindings/fnec_py/` | `test_smoke.py` | pyo3 `solve/sweep_deck_str` | ✅ |
| PH4-CHK-005 | GAP-004 | `plugin-api-design.md` | `nec_model` (EP-4) | `deck_validator.rs` | DeckValidator | ✅ |
| PH4-CHK-006 | GAP-012 | `automation-guide.md` | docs + example | frontmatter gate | automation guide | ✅ |
| PH4-CHK-007 | PRT-009, GAP-009 | `phase5-entry-criteria.md` | — | frontmatter gate | measurable GPU entry gate | ✅ |

### Phase 5 — GPU acceleration (complete, v0.5.0; gates G1–G7)

| ID | Req | Design | Impl | Tests | Result | S |
|:---|:----|:-------|:-----|:------|:-------|:-:|
| PH5-CHK-001 | GAP-007 | `gpu-arch.md` | — | — | wgpu chosen; G1–G7 defined | ✅ |
| PH5-CHK-002 | DEC-003 | `gpu-arch.md` | `nec_accel/wgpu_device.rs` | `nec_accel` unit | device enum + no-op pipeline | ✅ |
| PH5-CHK-003 | DEC-003 | `gpu-arch.md` | RP WGSL shader | `hallen_fr_cpu_reference.rs` | G3 RP parity | ✅ |
| PH5-CHK-004 | DEC-003 | `gpu-arch.md` | `nec-cli` `--exec gpu` | `gpu_rp_exec.rs` | G4 CLI RP parity | ✅ |
| PH5-CHK-005 | DEC-003 | `gpu-arch.md` | benchmark path | `gpu_benchmark_gate.rs` | G5 timing gate | ✅ |
| PH5-CHK-006 | DEC-003 | `gpu-arch.md` | `zmatrix_fill.wgsl` | `gpu_zmatrix_parity.rs` | G6 rel err 2.12e-6 | ✅ |
| PH5-CHK-007 | DEC-003 | `gpu-arch.md` | `nec_accel`, `nec_solver` | `gpu_hallen_solve.rs` | G7 ΔR=ΔX=0 Ω | ✅ |

### Phase 6 — scale-out & multi-vendor GPU (complete, v0.6.0)

| ID | Req | Design | Impl | Tests | Result | S |
|:---|:----|:-------|:-----|:------|:-------|:-:|
| PH6-CHK-001 | — | `benchmark-artifact-schema.md` | `benchmark-dashboard.yml` | benchmark gate | dashboard + gh-pages | ✅ |
| PH6-CHK-002 | PRT-009, CP-009 | `nec5-frontier.md` | corpus rows | `corpus_validation.rs` | wire-only; `PH6N5-*` | ✅ |
| PH6-CHK-003 | DEC-011 | `rooftop-basis-plan.md` | `nec_solver/basis.rs`, `linear.rs` | `sinusoidal_a2_regression.rs` | sinusoidal EFIE; warning retired | ✅ |
| PH6-CHK-004 | DEC-008, CP-009 | `multi-vendor-gpu.md` | `nec_accel` | wgpu parity tests | AMD Vulkan validated | ✅ |
| PH6-CHK-005 | PRT-011, CP-011 | `distributed-execution-design.md` | — | — | transport/authN design | ✅ |
| PH6-CHK-006 | PRT-011 | `worker-deployment.md` | `nec_worker/*` | `worker_integration.rs` | two-node solve match | ✅ |
| PH6-CHK-007 | PRT-011 | `distributed-execution-design.md` | `nec_worker/result_cache.rs` | `result_cache_contract.rs` | hit/miss/invalidation | ✅ |

### Phase 7 — GPU productionization (complete, v0.7.0)

| ID | Req | Design | Impl | Tests | Result | S |
|:---|:----|:-------|:-----|:------|:-------|:-:|
| PH7-CHK-001 | — | `ph7-chk-001-gpu-stub-retirement.md` | `nec_accel/gpu_kernels.rs`, `lib.rs` | `nec_accel` unit | scaffold retired; no fake GPU time | ✅ |
| PH7-CHK-002 | — | `ph7-chk-002-gpu-microbenchmark.md` | `wgpu_device.rs` (`microbench_*`) | `gpu_microbench.rs` | 61 ms init vs 268 µs dispatch; 10/10 | ✅ |
| PH7-CHK-003 | CP-011 | `ph7-chk-003-gpu-resident-solve.md` | `hallen_normal_solve.wgsl`, `solve_hallen_gpu_resident` | `gpu_resident_solve.rs`, `gpu_resident_solve_cli.rs` | ΔR=0.012 Ω vs f64 CPU | ✅ |
| PH7-CHK-004 | PRT-011, CP-011 | `ph7-chk-004-distributed-gpu-execution.md` | `nec_worker/protocol.rs`, `solve.rs` | `worker_gpu_exec.rs`, `gpu_exec.rs` | GPU node Δ≈0.009 Ω; CPU fallback | ✅ |
| PH7-CHK-005 | DEC-008, CP-009 | `ph7-chk-005-real-gpu-benchmark.md` | `examples/gpu_crossover.rs` | benchmark artifact | ~240× Z-fill at 1536 seg | ✅ |
| PH7-CHK-006 | DEC-008, CP-009 | `multi-vendor-gpu.md` | — | frontmatter gate | dated ROCm/SYCL deferral | ✅ |

### Phase 8 — deck-portability frontier (in progress)

| ID | Req | Design | Impl | Tests | Result | S |
|:---|:----|:-------|:-----|:------|:-------|:-:|
| PH8-CHK-001 | CP-003, PRT-002 | `ph8-chk-001-current-source.md` | `nec_solver/linear.rs` (`solve_hallen_current_source`), `excitation.rs` (`build_current_source_shape`), `nec-cli/solve_session.rs` (routing, feedpoint) | `nec_solver/tests/current_source.rs` (Z-consistency); `ex_cards.rs`/`corpus_validation.rs` (CLI accept-path, `dipole-ex4` Z-gate) | **Solve core** #260 (Z-consistency 2×10⁻⁴). **CLI wiring** 2026-07-02: EX type 4 solves on hallen (`FEEDPOINTS Z=V/i0`); `dipole-ex4` validates Z=74.23+j13.9; non-hallen fail fast; EX type 4 → Partial. **Non-junctioned multi-wire** 2026-07-02 (two-wire Z-consistency); junctioned pending. | 🔨 |
| PH8-CHK-002 | CP-003, PRT-002 | `ph8-chk-002-plane-wave-excitation.md` | `nec_model/card.rs`, `nec_parser` (F3), `nec_solver/planewave.rs` + `linear.rs` (2-DOF solve), `nec-cli/solve_session.rs` (routing, report) | `nec_solver/tests/planewave_nec2c.rs` (shape, symmetry, reciprocity); `ex_cards.rs`/`parser_warnings.rs` (CLI accept-path) | **Design** #255. **Code foundation** #257. **Solve core** #258 (nec2c shape 4.3%, reciprocity exact). **CLI wiring** #259: type-1 linear single-wire solves on hallen. **Elliptic** (types 2/3) 2026-07-02: complex polarization (axial ratio F6); z-wire/AR=0 reduce to linear, tilted-wire nec2c shape 5.4%; EX types 1/2/3 → Partial. **Multi-wire** (non-junctioned) 2026-07-02: per-wire forcing; two-wire nec2c shape ~10%, symmetric currents equal; 557 tests. Sweeps + junctioned multi-wire pending. | 🔨 |
| PH8-CHK-003 | CP-003, PRT-002 | roadmap row | `nec_solver/excitation.rs` | `ex_cards.rs` (+ fixture) | — | 📋 |
| PH8-CHK-004 | CP-003, PRT-002 | `ph8-chk-004-nt-network.md` | `nec_solver/network.rs` (`build_nt_stamps`, Y→Z), `nec-cli/solve_session.rs` (stamp application) | `nec_solver/tests/nt_network.rs` (TL-equivalence, guards); `corpus` `dipole-nt-tl-equiv` (end-to-end NT==TL) | **Stamp core** #262 (identical to TL stamp). **CLI wiring** 2026-07-02: stamps applied in solve; real fixture reproduces TL impedance (~1e-5 Ω); deferred warning removed; 550 tests. NT → Partial. Non-reciprocal breadth pending. | 🔨 |
| PH8-CHK-005 | CP-003, PRT-002 | roadmap row | `nec_solver/tl.rs` (lossy) | `tl_cards.rs` (+ fixture) | — | 📋 |
| PH8-CHK-006 | CP-002, PRT-001 | `nec4-support.md` | `nec_solver` ground path | `ground_diagnostics.rs`, corpus | — | 📋 |

---

## In-flight focus: PH8-CHK-002

Full chain for the item currently being implemented:

- **Requirement**: CP-003 (missing source cards break deck portability),
  PRT-002. Roadmap `PH8-CHK-002`.
- **Design / decisions**: `docs/ph8-chk-002-plane-wave-excitation.md` — two
  decisions: (1) **align EX-type numbering to NEC2** (type 1 = plane wave; current
  source → type 4), user-approved 2026-06-27; (2) plane-wave RHS lives in the
  integral-equation **forcing term** (`exp(-jk_s s)` closed form for straight
  wires), not the delta-gap RHS.
- **Implementation** (staged): ✅ *code foundation* — `ExcitationKind` NEC2
  classifier + `ExCard.polarization_deg` F3 + accurate reject diagnostic.
  ✅ *solve core* — `nec_solver::planewave` + `solve_hallen_planewave` (2-DOF,
  isolated). ✅ *CLI wiring* — `nec-cli::solve_session` routes single-straight-wire
  linear plane waves to the solve (induced `CURRENTS`, no feedpoint); elliptic,
  multi-wire, and non-Hallén decks fail fast. ⏳ *pending* — elliptic (types 2/3),
  NTHETA/NPHI sweeps, multi-wire geometry.
- **Tests**: `apps/nec-cli/tests/ex_cards.rs` extended; new corpus fixture
  `dipole-ex2-planewave-*`; external `nec2c` parity + internal Rayleigh–Carson
  reciprocity gates.
- **Tooling / reference**: `docs/dev/ph8-planewave-ref-theta30.nec` (`nec2c`
  induced-current reference, needs `XQ`), plus the reciprocity cross-check against
  the validated RP far-field path.
- **Result**: design **#255**, code foundation **#257**, solve core **#258**
  (nec2c shape 4.3%, reciprocity exact). CLI wiring 2026-07-02 — single-straight-
  wire linear plane waves solve on `--solver hallen` with induced `CURRENTS`;
  elliptic/multi-wire/non-Hallén fail fast; type-1 → Partial; 544 tests passing,
  clippy clean. Remaining: elliptic polarization, angle sweeps, multi-wire.

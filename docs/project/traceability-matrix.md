---
project: fnec-rust
doc: docs/project/traceability-matrix.md
status: living
last_updated: 2026-07-10
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
| FR-003 execute real NEC decks | Phases 1–2, Phase 8 (EX 0–5, NT, TL, ground) | ✅ (PT deferred) |
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
| PRT-001 ground modeling | Phase 2 + PH8-CHK-006 (finite-ground RP); Sommerfeld → Phase 9 | 🔨 (Phase 9) |
| PRT-002 loads/TL/network/source | Phase 2 + PH8-CHK-001..005 | ✅ |
| PRT-003 sweep/gain/pattern/report | Phase 1–2 | ✅ |
| PRT-004 GUI/workflow | Phase 3 | ✅ |
| PRT-005/006 optimization/automation | Phase 3–4 | ✅ |
| PRT-007 geometry/embeddability | Phase 2–4 | ✅ |
| PRT-008 accuracy breadth | Phase 1–3 corpus | ✅ |
| PRT-009 NEC-5 surfaces | wire-only decision (`nec5-frontier.md`) | ✅ (decided) |
| PRT-010 NEC-5 validation matrix | PH2-CHK-007 | ✅ |
| PRT-011 distributed execution | Phases 6–7 | ✅ |
| CP-003 deck-portability cards | Phase 8 (EX 0–5, NT, lossy TL) | ✅ |
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

### Phase 8 — deck-portability frontier (complete 2026-07-04)

| ID | Req | Design | Impl | Tests | Result | S |
|:---|:----|:-------|:-----|:------|:-------|:-:|
| PH8-CHK-001 | CP-003, PRT-002 | `ph8-chk-001-current-source.md` | `nec_solver/linear.rs` (`solve_hallen_current_source`), `excitation.rs` (`build_current_source_shape`), `nec-cli/solve_session.rs` (routing, feedpoint) | `nec_solver/tests/current_source.rs` (Z-consistency); `ex_cards.rs`/`corpus_validation.rs` (CLI accept-path, `dipole-ex4` Z-gate) | **Solve core** #260 (Z-consistency 2×10⁻⁴). **CLI wiring** 2026-07-02: EX type 4 solves on hallen (`FEEDPOINTS Z=V/i0`); `dipole-ex4` validates Z=74.23+j13.9; non-hallen fail fast; EX type 4 → Partial. **Non-junctioned multi-wire** 2026-07-02 (two-wire Z-consistency); junctioned pending. | 🔨 |
| PH8-CHK-002 | CP-003, PRT-002 | `ph8-chk-002-plane-wave-excitation.md` | `nec_model/card.rs`, `nec_parser` (F3), `nec_solver/planewave.rs` + `linear.rs` (2-DOF solve), `nec-cli/solve_session.rs` (routing, report) | `nec_solver/tests/planewave_nec2c.rs` (shape, symmetry, reciprocity); `ex_cards.rs`/`parser_warnings.rs` (CLI accept-path) | **Design** #255. **Code foundation** #257. **Solve core** #258 (nec2c shape 4.3%, reciprocity exact). **CLI wiring** #259: type-1 linear single-wire solves on hallen. **Elliptic** (types 2/3) 2026-07-02: complex polarization (axial ratio F6); z-wire/AR=0 reduce to linear, tilted-wire nec2c shape 5.4%; EX types 1/2/3 → Partial. **Multi-wire** (non-junctioned) 2026-07-02: per-wire forcing; two-wire nec2c shape ~10%, symmetric currents equal; 557 tests. Sweeps + junctioned multi-wire pending. | 🔨 |
| PH8-CHK-003 | CP-003, PRT-002 | `ph8-chk-003-ex-type5.md` | `nec_model/card.rs` (`is_voltage_source`), `nec_solver/excitation.rs` | `ex_cards.rs` (type-5 Z == type-0); `corpus` `dipole-ex5-*` | **Done** 2026-07-03: EX type 5 as voltage source (applied-field); Z == type 0; solves on hallen + pulse; 557 tests. NEC current-slope (~6%) documented non-goal. EX type 5 → Partial. | ✅ |
| PH8-CHK-004 | CP-003, PRT-002 | `ph8-chk-004-nt-network.md` | `nec_solver/network.rs` (`build_nt_stamps`, Y→Z), `nec-cli/solve_session.rs` (stamp application) | `nec_solver/tests/nt_network.rs` (TL-equivalence, guards); `corpus` `dipole-nt-tl-equiv` (end-to-end NT==TL) | **Stamp core** #262 (identical to TL stamp). **CLI wiring** 2026-07-02: stamps applied in solve; real fixture reproduces TL impedance (~1e-5 Ω); deferred warning removed; 550 tests. NT → Partial. Non-reciprocal breadth pending. | 🔨 |
| PH8-CHK-005 | CP-003, PRT-002 | `ph8-chk-005-lossy-tl.md` | `nec_solver/tl.rs` (lossy `coth/csch(γℓ)`) | `nec_solver/tests/lossy_tl.rs` (lossless limit, attenuation, matched-line) | **Done** 2026-07-04: lossy line stamp; F3=loss dB; reduces to lossless at 0 dB; 563 tests. TL other → Partial. | ✅ |
| PH8-CHK-006 | CP-002, PRT-001 | `ph8-chk-006-finite-ground-rp.md` | `nec_solver/farfield.rs` (Fresnel reflection far field) | `nec_solver/tests/finite_ground_rp.rs` (PEC limit, nec2c shape, horizon null) | **In progress** 2026-07-03: RP over finite ground via Fresnel coefficients (was free-space); PEC limit <0.05 dB, nec2c shape 0.053 dB; 560 tests. Directivity (gain offset documented). Buried/Sommerfeld = documented frontier deferral. | ✅ |

### Phase 9 — accuracy frontier & scattering breadth (planned, draft)

| Checklist ID | Req | Design | Impl | Tests | Result | S |
|:---|:----|:-------|:-----|:------|:-------|:-:|
| PH9-CHK-001 | CP-003, PRT-003 | `ph9-chk-001-receive-pattern.md` | `nec_model/card.rs` (ExCard F4/F5), `nec-cli/solve_session.rs` (`plane_wave_receive_sweep`), `nec_report` (`ReceivePatternRow`) | `nec-cli/tests/receive_pattern.rs` (sweep shape; reciprocity <0.01 dB) | **Done** 2026-07-04: NTHETA×NPHI receive sweep → RECEIVE_PATTERN; peak-current scalar matches transmit gain by reciprocity to <0.01 dB; 568 tests. | ✅ |
| PH9-CHK-003 | CP-002, PRT-001 | `ph9-chk-003-absolute-gain-ground.md` | `nec_solver/farfield.rs` (`radiation_efficiency`), `nec-cli/solve_session.rs` (gain scaling) | `nec_solver/tests/finite_ground_rp.rs` (lossless η≈1; absolute gain vs nec2c 0.06 dB) | **Done** 2026-07-04: gain = directivity + 10log10(η) over finite ground; matches nec2c absolute gain to 0.06 dB; free-space η=0.9996 validates the constant; 566 tests. Closes the PH8-CHK-006 directivity-vs-gain offset. | ✅ |
| PH9-CHK-005 | PRT-008/009/010 | `ph9-chk-005-junction-feed-guardrail.md` | `nec-cli/solve_session.rs` (`warn_if_feedpoint_at_junction`) | `nec-cli/tests/junction_feedpoint.rs` (junction-fed warns; fed-away / single-wire quiet) | **Done** 2026-07-04: characterized + guarded the junction-fed feedpoint limitation (split dipole fed at junction → −34−j1447 Ω vs true 74+j14 Ω); CLI warns, points to PH9-CHK-002; 571 tests. | ✅ |
| PH9-CHK-002 | CP-003, PRT-002/008 | `ph9-chk-002-junction-feed-diagnosis.md`, `ph9-chk-002-general-junction.md` | `nec_solver/geometry.rs` (`merge_collinear_wire_endpoints`, `build_conductor_paths`/`ConductorPath`, `classify_unsupported_topology`/`UnsupportedTopology`), `excitation.rs` (`build_hallen_rhs_paths`, `build_current_source_shape_paths`), `linear.rs` (`solve_hallen_paths`, `solve_hallen_planewave_paths`, `solve_hallen_current_source_paths`), `planewave.rs` (`build_planewave_hallen_paths`), `nec-cli/solve_session.rs` (path routing + `residual_hallen_paths` + `warn_if_unsupported_topology`) | `nec_solver/tests/collinear_merge.rs`, `nec_solver/tests/general_junction.rs` (split→single-wire exact; inverted-V R vs nec2c; path-decomposition units), `nec_solver/tests/planewave_junction.rs` (receive: split→per-wire exact; inverted-V reciprocity), `nec-cli/tests/receive_junction.rs` (CLI junctioned receive sweep + reciprocity), `nec_solver/tests/current_source_junction.rs` (EX-4 Z=V/i0 == voltage-source Z on split + inverted-V), `nec-cli/tests/current_source_junction.rs` (CLI junctioned EX-4 feedpoint Z), `nec-cli/tests/junction_feedpoint.rs` (degree-2 solves; degree-3 guarded) | **Partial** 2026-07-05: root cause = Hallén homogeneous basis resets per GW wire. Collinear case then **all degree-2 junctions** fixed via continuous conductor paths (signed arc-length `cos(k·s)` + per-path C): start-to-start split 74.41+j14.52 (was −34−j1447, == collinear), inverted-V 30°/45°/90° match nec2c radiation resistance to 2–4%; no-op elsewhere. **Receive-side plane-wave junctions** 2026-07-05: 2-DOF (cos/sin) path solver (`solve_hallen_planewave_paths` / `build_planewave_hallen_paths`) CLI-wired via `solve_plane_wave_hallen` — split-dipole receive == per-wire solver to ~1e-11, bent inverted-V reciprocity 1.5%, CLI split-dipole receive sweep matches transmit to 0.025 dB; 598 tests. **Current-source (EX type 4) junctions** 2026-07-06: `solve_hallen_current_source_paths` / `build_current_source_shape_paths` (one cos C/path + port V), CLI-wired via `solve_current_source_hallen` — split-dipole + inverted-V `Z=V/i0` == voltage-source Z to ~2–3e-4, CLI split-dipole EX-4 feedpoint Z matches voltage-source deck to ~2e-4; 602 tests. All three degree-2 excitation classes now on conductor paths. **Out-of-scope topology guard** 2026-07-06: `classify_unsupported_topology` + `warn_if_unsupported_topology` emit a whole-geometry warning for closed loops / degree-3+ T/Y regardless of feed placement (loop fed mid-wire was silent garbage: 1λ loop ≈20−j1210 vs nec2c 111−j146); closed-loop solve prototyped vs nec2c, periodic closure did not validate → deferred/guarded; 606 tests. Degree-3+ (T/Y) and closed-loop solves remain the last frontier. | 🔨 |
| PH9-CHK-006 | CP-002, PRT-001 | `ph9-chk-006-sommerfeld-ground.md` | `nec_solver/matrix.rs` (`image_segment` ground-image current direction), `nec-cli/solve_session.rs` (`warn_if_low_finite_ground`) | `nec_solver/tests/ground_impedance.rs` (ground ΔZ sign/magnitude vs nec2c, 3 geometries incl. Sommerfeld accuracy at 0.25λ); `nec-cli/tests/ground_diagnostics.rs` (low-height guard fires/quiet); refreshed corpus + ground_diagnostics ground regressions | **Partial** 2026-07-08: fixed the sign-inverted Z-matrix ground image `(Jx,Jy,−Jz)`→`(−Jx,−Jy,+Jz)` (matches the correct far-field image); every near-ground impedance had the wrong-signed ground effect (horizontal dipole 0.1λ 92−j48 vs nec2c 52+j63). Validated by ΔZ vs nec2c: ΔR sign correct across 4 geometries, near-ground vertical +18.0 vs +18.0, PEC external-R 7→0.93 Ω. **Boundary + guard**: accurate (≈ Sommerfeld GN2, gated 0.25λ +9.9 vs +11.0) for heights ≥ ~0.2λ; `warn_if_low_finite_ground` warns < 0.1λ (reflection-coefficient regime, no surface wave); 612 tests. Meets acceptance criteria. Angle-dep Fresnel RCM not worth a slice (fnec ≈ RCM already); Sommerfeld/Norton surface wave (the < 0.1λ fix) deferred. | 🔨 |
| PH9-CHK-007 | CP-003, PRT-002/008, CP-002, PRT-001 | `mpie-solver-scope.md` | `nec_solver/mpie.rs` (`MpieWire`, `assemble_free_space_z`, `solve_mpie_free_space`, `straight_wire`), `linear.rs` (`solve_square_in_place` reused) | `nec_solver/tests/mpie_free_space.rs` (A2 λ/2 convergence plateau; A3 orientation invariance; current symmetry/taper) + `mpie.rs` unit tests (A1 Z symmetry; short-dipole capacitive X; oracle N=40 match) | **Phase A done** 2026-07-09: free-space MPIE straight-wire core — triangle basis, Galerkin double-quadrature reduced-kernel `Z`, delta-gap `V`, direct dense solve; Rust port of the validated Python oracle `efie_mpie_ground.py::zmat_free`. A1 `Z` symmetric to machine precision (upper-triangle fill + mirror); A2 λ/2 dipole R monotone 73.4→74.4→75.5 toward 79.35 (matches oracle's ~6% offset, no Hallén reactance bias); A3 reversed-node/mirror-feed Z identical. Second solver, opt-in, Hallén corpus untouched. **Phase B degree-N junctions done** 2026-07-09: refactored to a wire-graph leg-based triangle basis (`MpieGeometry`/`build_bases`/`solve_mpie`); degree-N junction node → N−1 arm-pair dipole bases so KCL is automatic. B1 headline: Y-junction (nec2c 71.5) R converges MONOTONICALLY (N=10/20/40 → 68.75/69.33/69.84, within 5%, step <1) — the case the entire-domain Hallén prototype diverged on (R past 80); validated vs live nec2c + oracle `studies/.../mpie_junction.py`. B4 structural + degree-2 bend gated (`tests/mpie_junction.rs`). **B3 closed loops done** 2026-07-09: a cyclic all-degree-2 graph flows through the same basis with NO solver change (Hallén periodic closure never validated). 1λ square loop R+X converge to live nec2c 109.7−j146.2; small loop R matches analytic 20π²(C/λ)⁴ (`tests/mpie_loop.rs`). Phase B complete (junctions + loops). **Phase C far-field done** 2026-07-09: `segment_currents`/`segments_for_farfield` feed recovered currents into the existing `compute_radiation_pattern` (no new far-field code). C1: λ/2 dipole 2.15 dBi + analytic shape <0.1 dB; planar Y 1.94 dBi = live nec2c. C2: pattern symmetric + azimuthally invariant (`tests/mpie_farfield.rs`). **Phase D Sommerfeld-in-Z done** 2026-07-10 — RETIRES the 3rd frontier (near-ground currents/patterns): `sommerfeld::reflected_potential_kernels` (G_A, G_Φ) added to the MPIE Z-fill via ρ-grid precompute (`assemble_with_ground`/`solve_mpie_ground`). D1 horizontal λ/2 dipole GN2 reproduces the oracle to the digit (0.05λ 64.00+j49.18, 0.025λ 83.46+j66.26; nec2c 67.26+j52.61/87.81+j68.64, <8%); D2 PEC cancellation R=5.87; D3 kernel vs oracle ~1e-3 (`tests/mpie_ground.rs`, `sommerfeld.rs`). **Phase E CLI wiring done** 2026-07-10: `--solver mpie` (`cli_args.rs` + `SolverMode::Mpie`) exposes the MPIE — `mpie::geometry_from_segments`/`feed_node_for_segment` bridge the deck to the graph, `solve_session::solve_mpie_session` returns segs-aligned currents so the existing report path is reused (Hallén branches untouched, zero regression). λ/2 dipole 74.4+j41.8 (reactance tracks nec2c 44.7, unlike Hallén +5.1); degree-3 Y solves R≈64 (Hallén garbage R≈8); LD/TL/NT + plane-wave/current-source rejected (`apps/nec-cli/tests/mpie_solver_cli.rs`). **General orientation over ground done** 2026-07-10: reflected term for a vertical/tilted straight wire = Galerkin reaction of `reflected_e_projected_fast` added to the free-space Z (`add_general_ground_reaction`; the E-field reaction == potential-form Z, so it adds directly). E1 vertical λ/2 dipole GN2 84.5+j36 vs nec2c 89.75+j38.52; E2 near-horizontal reproduces Phase-D 64.00+j49.18. **Bent over ground done** 2026-07-10: `add_bent_ground_reaction` precomputes the reflected kernel per segment pair at midpoints (nseg² `reflected_e_projected_fast`) + analytic triangle overlap; E3 inverted-V over GN2 47.8+j32.6 (nec2c 52.81+j39.16, R <9%, ΔX matches <15%), E4 buried wire rejected. `tests/mpie_ground.rs`, study `mpie_general_ground.py`. **PH9-CHK-007 COMPLETE** (3 frontiers retired, user-accessible, any orientation straight/bent over ground; only buried rejected). | ✅ |


Drafted 2026-07-04 (`docs/roadmap.md` "Phase 9"). Six planned items (📋): angle
sweeps + receive pattern (PH9-CHK-001), junctioned multi-wire receive solves
(002), absolute gain over lossy ground (003), PT + full RP output modes (004),
difficult-geometry accuracy corpus (005), first Sommerfeld/buried near-ground
increment (006). Theme ordering and first-frontier priority are a **product
decision** — matrix rows land here as each item is scheduled.

---

## Current status (2026-07-04)

- **Released**: **v0.8.0** — Phase 8 complete (mainstream deck portability). All
  six PH8-CHK items delivered and validated; every EX source card (0–5), NT
  networks, lossy TL, and the finite-ground radiation pattern are user-runnable.
- **Latest tests**: 564 passing, clippy clean (see [test-results.md](test-results.md)).
- **Next**: **Phase 9 (planned, draft)** — accuracy frontier & scattering breadth
  (`docs/roadmap.md` "Phase 9"). The first-frontier priority (receive-side breadth
  vs advanced ground vs difficult-geometry accuracy) is a product decision; no
  PH9 item is scheduled yet.
- **Open frontier deferrals** (each with a recorded blocker): junctioned-multi-wire
  plane wave, NTHETA/NPHI sweeps, buried/Sommerfeld ground, non-reciprocal NT,
  absolute gain over lossy ground — all folded into the Phase 9 draft.

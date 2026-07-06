---
project: fnec-rust
doc: docs/project/test-catalog.md
status: living
last_updated: 2026-07-06
---

# Test catalog

The **tests layer**: every test file, its function count, what it validates, and
the checklist/requirement it gates. Counts are `#[test]`/`#[tokio::test]` function
counts (measured, not estimated). Aggregate pass/fail is recorded separately in
[test-results.md](test-results.md).

## Integration / contract tests

| Test file | # | Validates | Gates |
|:----------|:--|:----------|:------|
| `apps/nec-cli/tests/core_flags_contract.rs` | 15 | `--solver`/`--pulse-rhs`/`--exec` flag contract + usage errors | NFR-005, PH2-CHK-008 |
| `apps/nec-cli/tests/corpus_deck_sanity.rs` | 1 | Every corpus `.nec` deck has a `GE` card | Corpus hygiene |
| `apps/nec-cli/tests/corpus_validation.rs` | 8 | Golden corpus matches references; checklist coverage (PAR002/003/005, loaded, pattern) | NFR-004, COMP-002/008, PH2-CHK-005/007 |
| `apps/nec-cli/tests/deck_validator.rs` | 4 | Deck validator warns on missing `EX`; silent on well-formed decks | FR-009, EP-4 |
| `apps/nec-cli/tests/ex_cards.rs` | 9 | `EX` types 0/1/3 feedpoint parity; unsupported types rejected | CP-003, PH8-CHK-001/002 (baseline) |
| `apps/nec-cli/tests/exec_modes.rs` | 24 | `--exec` selection, drop-in alias resolution, sandbox paths | DEC-003, CP-012 |
| `apps/nec-cli/tests/geometry_diagnostics.rs` | 3 | Fail-fast on crossing wires / tiny source; valid junctions accepted | FR-009, PH2-CHK-006 |
| `apps/nec-cli/tests/gpu_benchmark_gate.rs` | 1 | Gate G5: GPU exec ≤1.5× CPU on large RP grid (best-of-N) | PH5-CHK-005, PH7-CHK-002 |
| `apps/nec-cli/tests/gpu_resident_solve_cli.rs` | 1 | `--exec gpu` feedpoint Z within 2 Ω of CPU on corpus | PH7-CHK-003 |
| `apps/nec-cli/tests/gpu_rp_exec.rs` | 2 | Gate G4: `--exec gpu` RP far-field matches CPU | PH5-CHK-004 |
| `apps/nec-cli/tests/ground_diagnostics.rs` | 10 | `GN`/`GE` handling: PEC inference, GN0/GN2 active, GN3 deferred | PRT-001, PH2-CHK-001/002 |
| `apps/nec-cli/tests/hallen_fr_cpu_reference.rs` | 6 | Hallén FR CPU reference kernel (wgpu RP parity baseline) | PH5-CHK-003, PH7-CHK-001 |
| `apps/nec-cli/tests/json_output_contract.rs` | 5 | JSON output valid/stable, required fields, sweep records | FR-008, PH4-CHK-003 |
| `apps/nec-cli/tests/ld_loads.rs` | 5 | `LD` types 1/2/4 change impedance; unsupported warn+continue | PRT-002, PH2-CHK-003 |
| `apps/nec-cli/tests/loaded_case_tracking.rs` | 2 | Loaded non-collinear topology solves; `--allow-noncollinear` no-op | DEC-010 |
| `apps/nec-cli/tests/parser_warnings.rs` | 22 | Warnings for unknown cards, `TL` types/segments; runs still succeed | COMP-001, PRT-002 |
| `apps/nec-cli/tests/report_contract.rs` | 5 | Report v1 headers/rows; RP/sweep/load tables; section ordering | FR-005, PH2-CHK-004 |
| `apps/nec-cli/tests/resonance_contract.rs` | 3 | `--resonance` convergence, unbounded fail, missing-flag usage | FR-010, PH3-CHK-008 |
| `apps/nec-cli/tests/result_cache_contract.rs` | 5 | Distributed result cache hit/miss/invalidation + sweep reuse | PH6-CHK-007 |
| `apps/nec-cli/tests/scriptability_contract.rs` | 25 | Scripting/drop-in alias contract; temp-file & path handling | NFR-005, GAP-011, PH2-CHK-008 |
| `apps/nec-cli/tests/sinusoidal_a2_regression.rs` | 2 | Sinusoidal solver tracks Hallén on dipole + sweep | DEC-011, PH6-CHK-003 |
| `apps/nec-cli/tests/sweep_contract.rs` | 5 | Sweep point/list/linear produce correct frequency blocks | FR-007, PH3-CHK-006 |
| `apps/nec-cli/tests/template_contract.rs` | 5 | TOML/JSON var substitution; undefined-token error | PH3-CHK-007 |
| `apps/nec-cli/tests/tl_cards.rs` | 3 | `TL` card changes feedpoint Z across nseg | PRT-002, PH2-CHK-003 |
| `apps/nec-cli/tests/topology_fallback.rs` | 13 | Non-single-chain fallback across solver/pulse/exec/sinusoidal/loaded | DEC-010/011 |
| `apps/nec-cli/tests/worker_gpu_exec.rs` | 1 | Distributed GPU dispatch through worker pool (mixed gpu/cpu) | PH7-CHK-004 |
| `apps/nec-cli/tests/worker_integration.rs` | 7 | Hosts config, capability cache, subprocess round-trip | PH6-CHK-006/007 |
| `apps/nec-gui/tests/gui_smoke.rs` | 47 | Headless GUI state machine + solve pipeline | PRT-004, PH3-CHK-009/010/011 |
| `crates/nec_accel/tests/gpu_hallen_solve.rs` | 1 | Gate G7: GPU Z-fill + CPU Hallén solve end-to-end | PH5-CHK-007 |
| `crates/nec_accel/tests/gpu_microbench.rs` | 1 | Microbench separates per-dispatch time from device init | PH7-CHK-002 |
| `crates/nec_accel/tests/gpu_resident_solve.rs` | 1 | Fully GPU-resident Hallén fill+solve parity | PH7-CHK-003 |
| `crates/nec_accel/tests/gpu_zmatrix_parity.rs` | 1 | Gate G6: GPU Z-fill element-wise parity vs CPU | PH5-CHK-006 |
| `crates/nec_project/tests/project_roundtrip.rs` | 20 | `ProjectFile` TOML/Markdown round-trip + errors | FR-004, PH3-CHK-004/005, GAP-015 |
| `crates/nec_solver/tests/pulse_rhs_scaling.rs` | 1 | Pulse RHS inverse-wavelength scaling | PRT-002 |
| `crates/nec_solver/tests/planewave_junction.rs` | 2 | Receive-side degree-2 junction solve: split-dipole receive == per-wire solver (~1e-11); bent inverted-V reciprocity 1.5% | PH9-CHK-002 |
| `crates/nec_solver/tests/current_source_junction.rs` | 3 | Current-source (EX type 4) degree-2 junction solve: split-dipole + inverted-V Z=V/i0 == voltage-source Z (~2–3e-4); i0 linearity | PH9-CHK-002 |
| `apps/nec-cli/tests/receive_junction.rs` | 2 | CLI junctioned receive: split-dipole receive sweep has dipole shape and matches transmit by reciprocity (0.025 dB) | PH9-CHK-002 |
| `apps/nec-cli/tests/current_source_junction.rs` | 1 | CLI junctioned current source: split-dipole EX-4 feedpoint Z=V/i0 matches voltage-source Z (~2e-4) | PH9-CHK-002 |
| `crates/nec_worker/tests/gpu_exec.rs` | 2 | Worker-level GPU execution vs CPU parity | PH7-CHK-004 |

Integration subtotal: **266** test functions (nec-cli 192, nec-gui 47,
nec_accel 4, nec_project 20, nec_solver 1, nec_worker 2).

## Unit tests (in `src/`)

| Crate | # `#[test]` | Concentration |
|:------|:------------|:--------------|
| `nec_solver` | 100 | loads 18, geometry 20, excitation 15, linear 14, matrix 12, farfield 9, basis 6, tl 6 |
| `nec_worker` | 66 | worker 17, result_cache 13, solve 7, capability 7, protocol 6, hosts 6, pool 5, controller 3, ssh_worker 2 |
| `nec_report` | 25 | lib 25 |
| `nec_accel` | 25 | gpu_kernels 20, lib 5 |
| `nec_parser` | 21 | lib 14, template 7 |
| `nec_project` | 12 | lib 12 |
| `apps/nec-cli` | 10 | main 10 |
| `nec_model` | 7 | lib 7 |

Unit subtotal: **266** `#[test]` functions.

## Totals

- **`#[test]` functions**: ~532 (266 integration + 266 unit).
- **`cargo test --workspace` aggregate** (includes doctests): **539 passing** across
  53 test binaries — the authoritative pass count in [test-results.md](test-results.md).
- **wgpu-gated GPU tests** (`cargo test -p nec_accel --features wgpu`): **29 passing**
  across 6 binaries (real device dispatch, not the software rasterizer).

The small gap between the `#[test]` grep count (532) and the `cargo test` aggregate
(539) is doctests and feature-gated variants, which `cargo test` runs but the grep
does not count.

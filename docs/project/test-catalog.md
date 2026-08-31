---
project: fnec-rust
doc: docs/project/test-catalog.md
status: living
last_updated: 2026-08-31
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
| `crates/nec_solver/tests/ground_impedance.rs` | 3 | Near-ground impedance: ground ΔZ vs nec2c — horizontal (R drops), vertical near-ground (R rises +18Ω), and 0.25λ vs Sommerfeld truth | PH9-CHK-006 |
| `apps/nec-cli/tests/receive_junction.rs` | 2 | CLI junctioned receive: split-dipole receive sweep has dipole shape and matches transmit by reciprocity (0.025 dB) | PH9-CHK-002 |
| `apps/nec-cli/tests/current_source_junction.rs` | 1 | CLI junctioned current source: split-dipole EX-4 feedpoint Z=V/i0 matches voltage-source Z (~2e-4) | PH9-CHK-002 |
| `crates/nec_worker/tests/gpu_exec.rs` | 2 | Worker-level GPU execution vs CPU parity | PH7-CHK-004 |

Integration subtotal: <!-- COUNT:INTEGRATION-SUBTOTAL=514 --> **514** test
functions across the `tests/` binaries listed above.

## Unit tests (in `src/`)

<!-- Counts below are CHECKED, not typed: `scripts/check-test-catalog-counts.py`
     re-derives every number in this section from `cargo test --workspace -- --list`
     and fails the build on drift. Re-measure with `--list-only`. -->

| Crate | # `#[test]` | Concentration |
|:------|:------------|:--------------|
| `nec_solver` | 232 | loads, geometry, excitation, linear, matrix, farfield, basis, tl |
| `nec_worker` | 103 | worker, result_cache, solve, capability, protocol, hosts, pool, controller, ssh_worker |
| `nec-gui` | 91 | app_state, model_doc, mesh, camera, solve |
| `apps/nec-cli` | 33 | main, exec_profile, sweep_config, warnings |
| `nec_parser` | 27 | lib, template |
| `nec_accel` | 26 | kernel_reference 20, lib 6 |
| `nec_report` | 25 | lib 25 |
| `nec_project` | 21 | lib 21 |
| `nec_model` | 7 | lib 7 |

Unit subtotal: <!-- COUNT:UNIT-SUBTOTAL=565 --> **565** `#[test]` functions.

## Totals

- **Test functions**: <!-- COUNT:WORKSPACE-TOTAL=1086 --> **1086** = 565 unit + 514 integration + **7 doctests**.
- **`cargo test --workspace` aggregate**: **1084 passing, 0 failed, 2 ignored**,
  measured 2026-08-31 — the authoritative pass count in [test-results.md](test-results.md).

Doctests are counted separately on purpose. `cargo test --workspace -- --list`
prints them under `Doc-tests <crate>` headers that carry no `Running` line, so a
parser that only tracks `Running` charges all seven to whichever `tests/*.rs`
binary happened to be listed last. The first version of the checker did exactly
that, inflating one row by 7 and the integration subtotal with it, and **passed**
— it was self-consistent with its own bug. Caught in review.

**The configuration matters, so it is stated rather than implied.** These numbers are
from a `--workspace` run. Feature unification turns on `nec_accel/wgpu` there, which
adds four `nec_accel` lib tests (26 rather than 22) *and* is the only reason that
crate's four `tests/*.rs` binaries compile at all — `cargo test -p nec_accel` alone
fails to build them (FND-144). A count is not a property of the tree by itself; it is
a property of the tree and the build configuration together.

These counts are derived, not typed: `scripts/check-test-catalog-counts.py` enumerates
what the harness will actually run and fails on drift. The previous figures — "~532
across 53 test binaries" — were hand-maintained and had drifted by more than a factor
of two while every one of them looked precise (FND-143).

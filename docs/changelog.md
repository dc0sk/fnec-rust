---
project: fnec-rust
doc: docs/changelog.md
status: living
last_updated: 2026-05-02
---

# Changelog

All notable documentation process changes are recorded here.

## [0.3.0] — 2026-05-01
### Added

- **PH3-CHK-010 (nec-gui sweep views)**: Added frequency-range sweep setup and result inspection views to `fnec-gui`. The GUI gains a Solve/Sweep tab bar switching between the existing single-frequency panel and a new sweep panel. The sweep panel provides Start/End/Step (MHz) text inputs, a Run Sweep button, a progress/status line, and a sortable four-column result table (Freq, Z_re, Z_im, |Z|). Column headers are clickable sort buttons with ascending/descending toggle indicators. Implementation: `app_state.rs` extended with `ActiveTab`, `SweepPhase`, `SweepSortCol`, `SweepSetup` fields, new `Message` variants (`TabSelected`, `SweepStartChanged`, `SweepEndChanged`, `SweepStepChanged`, `RunSweep`, `SweepComplete`, `SweepSortBy`), `can_sweep()`, `sweep_params()`, `sorted_sweep_rows()`, `sweep_status_text()`. `solve.rs` gains `SweepPoint` struct and `sweep_deck_str` / `sweep_deck_path` functions that build geometry once and iterate the impedance-matrix solve over each frequency. `main.rs` updated with tab bar, `sweep_view()`, `sweep_result_table()`, `sweep_row()` helpers. Added 14 new headless tests to `gui_smoke.rs` covering sweep state machine (8 tests) and sweep pipeline (5 tests), for a total of 27 smoke tests.

- **PH3-CHK-009 (nec-gui iced desktop window)**: Implemented the `fnec-gui` desktop frontend using `iced` 0.13. The binary presents a dark-themed window with a deck path text input, a Solve button, and a result panel showing frequency, Z_re, Z_im, and |Z|. The solve pipeline runs asynchronously via `Task::perform`. Implementation split: `apps/nec-gui/src/lib.rs` + `app_state.rs` (state machine — no iced dep, fully headless-testable) + `solve.rs` (Hallen solve wrapper calling `nec_solver` directly). Added 13 headless smoke tests in `apps/nec-gui/tests/gui_smoke.rs` covering state machine transitions (8 tests) and solve pipeline correctness (5 tests). Added `.github/workflows/gui-smoke.yml` CI gate running `cargo test -p nec-gui --test gui_smoke`.
### Added

- **PH3-CHK-008 (resonance-targeting helper)**: Added `fnec sweep --resonance <file.nec.toml>` subcommand that binary-searches one template variable to find the feedpoint reactance closest to a target (typically 0 Ω for series resonance). The `.nec.toml` file embeds both a `[search]` table (variable name, lo/hi bounds, target reactance, tolerance, max iterations) and a `[deck]` table containing the NEC template string. Implementation: `apps/nec-cli/src/resonance_search.rs` (`ResonanceFile` TOML struct, `bisect()` function, `print_result()`). Integrates with the template engine from PH3-CHK-007 and re-runs the full geometry/solve pipeline for each probe point. Added `examples/resonance-search.nec.toml` worked example (14.2 MHz dipole resonance search); added 3 contract tests in `apps/nec-cli/tests/resonance_contract.rs` (convergence, unbounded-range error, missing-flag usage error).

- **PH3-CHK-007 (variable-substitution engine)**: Added `nec_parser::template` module with a `substitute()` function that replaces `$VAR` tokens in NEC deck strings from a `HashMap<String, String>`. `$$` produces a literal `$`; undefined tokens return a `TemplateError` with the variable name and 1-based line number. CLI: `--vars <file>` flag loads a flat TOML or JSON key→value map and applies substitution before parsing. Added `apps/nec-cli/src/vars_config.rs` (TOML via `toml` crate; JSON via minimal hand-rolled parser). Added 5 contract tests in `apps/nec-cli/tests/template_contract.rs`. Corpus example: `corpus/variable-dipole.nec` (template) + `corpus/dipole-vars.toml` (vars). `--vars` documented in `docs/cli-guide.md` synopsis and options table.

- **PH3-CHK-006 (`--sweep-config` CLI flag)**: Added `--sweep-config <file.toml>` flag to the `fnec` binary. A TOML sweep-config file specifies a frequency list as either a linear range (`start_mhz`, `end_mhz`, `step_mhz`) or an explicit point list (`points_mhz = [...]`). When supplied, the sweep-config frequencies replace those derived from the deck's `FR` card; the full solve pipeline runs once per point and emits one structured output block per frequency on stdout. Implementation: `apps/nec-cli/src/sweep_config.rs` (TOML reader + validation); `apps/nec-cli/Cargo.toml` gains `serde` and `toml` workspace deps; `apps/nec-cli/tests/sweep_contract.rs` adds 5 contract tests (single-point explicit, multi-point explicit, range point-count, ordering stability, machine-parseability); `examples/sweep-spec.toml` provides a range-based reference example.

- **PH3-CHK-005 (run history API)**: Extended `nec_project` with `RunHistory` (transparent `Vec<RunRecord>`), `RunRecord` (ISO 8601 timestamp, `SolverConfig` snapshot, `ResultSummary`), and `ResultSummary` (impedance Re/Im, optional peak gain dBi, sweep point count). `ProjectFile` gains `run_count()`, `last_run()`, and `run_by_index()` query methods plus `RunHistory::push`. History is absent from TOML when empty; `peak_gain_dbi` is omitted when `None`. 5 history tests added (13 integration + 1 doctest total).

- **PH3-CHK-004 (nec_project TOML format)**: Implemented `ProjectFile`, `SolverConfig`, and `NamedRun` structs with serde/toml round-trip in `crates/nec_project/src/lib.rs`. Public API: `ProjectFile::from_toml` / `to_toml`; `ProjectError` with version-guard (`UnsupportedVersion`). 8 integration tests + 1 doctest in `crates/nec_project/tests/project_roundtrip.rs`. Project TOML format documented in `docs/project-format.md`.

- **PH3-CHK-003 (plugin API design)**: Added `docs/plugin-api-design.md` covering the extension surface, safety model (no network/filesystem/FFI through the trait interface), pipeline diagram, and future EP-3..5 scope. Implemented two working extension points: `DeckPostProcessor` trait (EP-1) in `crates/nec_model/src/lib.rs` (called after parse, before geometry build) and `ResultFilter` trait (EP-2) in `crates/nec_report/src/lib.rs` (called after solve, before report rendering). Both are exercised by doctests. BLK-004 updated to resolved.

- **PH3-CHK-002 (contributing guide)**: Added `docs/contributing.md` covering build workflow, pre-push sequence (`cargo fmt` → `cargo check` → `cargo test`), branch conventions, PR process, corpus-gate requirements, documentation frontmatter rules, and architecture orientation for new contributors. Added contributor orientation cross-references to `docs/architecture.md` and `docs/design.md`. The `validate-doc-frontmatter` CI gate picks up the new file automatically via its existing `docs/*.md` glob.

- **PH3-CHK-001 (card-status index)**: Added `## PH3-CHK-001 complete card status index` section to `docs/nec4-support.md` with a 25-row flat table listing every known NEC-2/NEC-4 mnemonic, its parser status (`recognized` / `unknown`), and functional status. Documents the GM/GR gap (geometry builder implemented but parser not yet wired). `par001_card_status_table_complete` test in `apps/nec-cli/tests/corpus_validation.rs` enforces all 12 parser-recognized mnemonics and 3 out-of-scope entries are present in CI.

- **Non-collinear multi-wire Hallen support (Phase 2)**: The Hallen solver now handles junctioned and non-collinear multi-wire topologies (e.g. `dipole-loaded` top-hat geometry, inverted-V, Yagi with passive elements) via a segmented hybrid reformulation:
  - `build_hallen_rhs` now computes per-wire local cos(k·s) homogeneous vectors using each wire's own midpoint as s=0, replacing the old global s-axis.
  - Passive (non-driven) wires receive rhs=0; all EX cards contribute to the source map (multi-source support).
  - `detect_wire_junctions()` in `geometry.rs` identifies shared wire endpoints; `solve_hallen` enforces KCL continuity rows for junction segments instead of the default I=0 endpoint condition.
  - `--allow-noncollinear-hallen` flag is now silently accepted (no-op) rather than deferred; non-collinear geometries are supported by default.
  - `dipole-loaded` corpus gate now passes: Z ≈ 12.39 − j918 Ω (external NEC2 reference: 13.46 − j896 Ω).
  - References for TL-coupled multi-dipole cases and Yagi 5-element case updated to reflect correct passive-wire rhs=0 behavior.

### Changed

- Extracted geometry validation helpers (`sinusoidal_a4_topology_supported`, `segment_intersection_error`, `source_risk_geometry_error`, `buried_wire_geometry_error`, and private math/graph helpers) into `apps/nec-cli/src/geometry_validation.rs`, and extracted all warning functions into `apps/nec-cli/src/warnings.rs`. `main.rs` is now reduced to frontend wiring, enums/constants, bench-emit helpers, and `fn main()`.
- Extracted per-frequency solve-session logic from `apps/nec-cli/src/main.rs` into a new `apps/nec-cli/src/solve_session.rs` module: all math helpers (`l2_norm`, `matrix_diagonal_spread`, `residual_zi_minus_v`, `residual_hallen`), pulse-source constraint helpers, report builders (`build_feedpoint_rows`, `build_source_rows`, `build_load_rows`), frequency/dispatch helpers (`frequencies_from_fr`, `build_hybrid_lane_plan`), all four structs (`FrequencySolveResult`, `SweepPointSummary`, `PulseCurrentSourceConstraint`, `HybridLanePlan`), and `solve_frequency_point` now live in `solve_session`. The function gains an explicit `sinusoidal_topology_supported: bool` parameter, computed once in `main()` before the solve closure, replacing the internal call to `sinusoidal_a4_topology_supported` inside the solve path.

- Continued CLI decomposition by extracting execution-profile policy logic (4nec2 drop-in detection/steering and startup auto-probe mode selection) from `apps/nec-cli/src/main.rs` into `apps/nec-cli/src/exec_profile.rs`.
- Started three accepted review follow-ups: parser fuzz scaffolding now exists under `fuzz/`, CLI argument parsing/usage text now lives in `apps/nec-cli/src/cli_args.rs`, and `nec_solver` now carries a first property-based Hallen reciprocity invariant test.
- Review follow-up triage now assigns owners and concrete closure criteria for the remaining GAP items, adds measurable Phase 3 usability minima, documents experimental residual budgets and the scoped GN0/GN2 finite-ground validity envelope, and starts documenting crate-level public surfaces for `nec_report` and `nec_project`.
- Report contract coverage now locks combined sweep-plus-operator-table ordering on stdout: multi-frequency runs with `LD` cards must emit one full per-frequency block in `FEEDPOINTS -> SOURCES -> LOADS -> CURRENTS` order before the final `SWEEP_POINTS` summary.
- Added a supported low above-ground GN2 near-ground corpus contract (`dipole-gn2-near-ground-51seg`) and tightened PH2-CHK-002 docs/tests so supported near-ground coverage is distinguished from buried active-ground fail-fast guardrails.
- Geometry diagnostics now also fail fast for source-risk tiny segments: `EX` requests on `L/r < 2` emit an actionable deferred-class error before solve.
- GN type 0 is now active as a simple finite-ground model in Hallen impedance assembly (complex Fresnel-style image scaling from EPSE/SIG) instead of the prior deferred free-space fallback warning path.
- Phase 2 current/phase corpus coverage now includes both `dipole-freesp-51seg` and `dipole-ground-51seg`, so CI locks representative free-space and PEC-ground current magnitude/phase samples instead of only the base dipole case.
- EX type 1 now has a first real implementation slice for `--solver pulse`: the pulse solver enforces the requested driven-segment current and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still keep the staged portability fallback warning.
- EX type 2 is now accepted as a staged portability fallback: the CLI warns that incident-plane-wave semantics are still pending, and current runtime behavior treats EX type 2 like EX type 0 until a dedicated implementation lands.
- EX type 4 now has a first real implementation slice for `--solver pulse`: the pulse solver enforces the requested driven-segment current and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still keep the staged portability fallback warning.
- EX type 5 now has a first real implementation slice for `--solver pulse`: the pulse solver enforces the requested driven-segment current and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still keep the staged portability fallback warning.
- TL `NSEG>1` cards for lossless lines (`type=0`) are now accepted in the executable network subset using the same uniform-line stamp semantics as `NSEG=1`; the previous deferred "TL with NSEG=... not yet supported" runtime warning path is removed.
- Phase 2 traceability coverage is now stricter: the enforced PH2-CHK-007 matrix explicitly maps newer EX current-source, LD load-family, TL subset, and PT/NT deferred-portability corpus classes, and CI now requires those row IDs to remain present.
- PT cards are now parsed for staged portability and emit an explicit deferred-support warning at runtime; PT electrical semantics are still pending and currently ignored.
- NT cards are now parsed for staged portability and emit an explicit deferred-support warning at runtime; NT electrical semantics are still pending and currently ignored.
- CLI report contract v1 now includes stable operator tables for source/load definitions: `SOURCES` (`TYPE TAG SEG I4 V_RE V_IM`) and `LOADS` (`TYPE TAG SEG_FIRST SEG_LAST F1 F2 F3`) sections, emitted in deterministic order between `FEEDPOINTS` and `CURRENTS`.
- Scriptability contracts now explicitly lock stdout ordering around the new tables (`FEEDPOINTS -> SOURCES -> CURRENTS`) and enforce that `LOADS` table output stays report-only on stdout while warnings remain stderr-only.
- Loaded-case tracking now also locks the default Hallen hard-fail contract on `dipole-loaded` (non-collinear topology error, exit code 1, and no report on stdout) to keep Phase 1 gap behavior explicit and deterministic.

### Added

- RP card execution is now wired into the CLI report path.
- Text reports now include a `RADIATION_PATTERN` section when one or more `RP` cards are present.
- Added corpus regression deck `corpus/dipole-freesp-rp-51seg.nec` and contract coverage for pattern-table rendering.
- Added `docs/benchmarks.md` with a validated three-host baseline comparison (local workstation, T480, Raspberry Pi 5).
- Added a collaboration efficiency guide with rate-limit-aware prompting patterns at `docs/copilot-efficiency-guide.md`.
- Added `docs/par011-dropin-evidence-memo.md` as a dedicated evidence scaffold for deferred 4nec2 drop-in compatibility work.
- **GPU kernel stubs** (Phase A expansion): Extended `nec_accel::gpu_kernels` module with additional kernel scaffolds:
  - `HallenRhsGpuKernel` for Hallén RHS vector computation with excitation handling
  - `PocklingtonMatrixGpuKernel` for matrix assembly with segment-pair element distribution
  - `KernelTiming` struct for capturing prep/exec/retrieval timing data (microsecond resolution)
  - 4 new unit tests for kernel construction and sizing (12 total nec_accel lib tests)
  - GPU-compatible data structures prepared for future CUDA/OpenCL replacement
- **CLI GPU FR integration** (Phase B): Added `--gpu-fr` command-line flag to dispatch radiation pattern computation to GPU kernel stub:
  - Far-field points routed through `HallenFrGpuKernel` when flag is enabled
  - Maintains full output parity with CPU far-field path
  - Integration tested with 6 GPU stub tests + existing exec_modes contract tests
- **Performance benchmarking** (Phase D): Added optional timing instrumentation for GPU kernel operations:
  - `--bench` CLI flag to enable benchmarking mode
  - `--bench-format <human|csv|json>` to emit machine-readable benchmark records while preserving the standard human-readable report output
  - `FNEC_GPU_BENCH` environment variable control (set to "1" to enable timing collection)
  - `compute_hallen_fr_point_with_timing()` API returns `(result, KernelTiming)` tuples
  - Timing breakdown: prep (coordinate transform), exec (far-field summation), retrieval (stub: zero)
  - Ready for future GPU timing collection once real CUDA/OpenCL kernels are wired
- Corpus validation framework already supports pattern and current-gate scenarios (Phase C); enhancements documented for future use.

### Changed

- Added missing `GE` cards to three corpus decks (`dipole-ld-series-rc-51seg`, `dipole-ld-series-rl-51seg`, `tl-two-dipoles-linked-seg0`) so `corpus_deck_sanity` passes consistently in local hooks and CI.
- Native CLI startup now auto-selects execution mode when `--exec` is omitted by running a quick execution probe (CPU threads, frequency-point count, and accelerator dispatch availability) and choosing among `cpu`/`hybrid`/`gpu` heuristically for the current workload shape.
- Consolidated benchmark documentation into a single canonical file (`docs/benchmarks.md`) and removed the duplicate `docs/benchmark.md` shim.
- Benchmark docs now explicitly map reported numbers to four execution modes: CPU single-thread, CPU multithread, GPU, and hybrid (CPU multithread + GPU), with a dedicated local four-mode coverage result block.
- Sinusoidal topology gating advanced through A4: the solver now accepts collinear wire-chain geometries (including multi-wire chains) with orientation/order-agnostic endpoint connectivity checks, and still falls back for disconnected/branched/unsupported topologies.
- Added a gitignored benchmark host env pattern (`.benchmark-hosts.env` with tracked `.benchmark-hosts.env.example`) and updated `scripts/pi-remote-benchmark.sh` to accept env defaults (`FNEC_BENCH_TARGET`, `FNEC_REMOTE_REPO_SUBDIR`).
- Remote benchmark tooling now supports execution-mode sweeps (`FNEC_BENCH_EXECS`) and records `diag_spread` plus `sin_rel_res` in benchmark CSV output and comparison reports.
- Added `scripts/pi-benchmark-summary.sh` to summarize a single benchmark CSV without pandas or ad hoc shell commands.
- Added `sin_rel_res` to CLI diagnostics: the sinusoidal basis relative residual captured before any fallback decision, enabling solver-quality trending across runs (0.0 for non-sinusoidal modes).
- Added `diag_spread` to CLI diagnostics as a conditioning proxy (ratio of max/min diagonal magnitudes of the solved system matrix), enabling quick stability checks in automation.
- Added sinusoidal A2 regression checks that compare sinusoidal-mode impedance output against Hallen on `dipole-freesp-51seg` and `frequency-sweep-dipole` corpus decks.
- Sinusoidal solver routing is now topology-gated for A1: it runs only on single-wire collinear decks and otherwise falls back explicitly to pulse with `sinusoidal->pulse(topology)` diagnostics.
- Completed PAR-008 coverage-matrix scope: NEC-5 validation scenario classes are now explicitly mapped to current corpus-backed in-scope equivalents, with out-of-scope classes and rationale documented for phased deferral.
- Updated support and CLI docs to mark RP pattern output as implemented in the text-report path (with remaining export/near-field scope still deferred).
- Corpus validation now numerically checks stored RP pattern samples instead of only asserting pattern-table presence.
- Corpus validation now also checks the stored vertical/horizontal gain columns and axial ratio for locked RP sample angles.
- RP corpus angle coverage was expanded from 2 locked sample angles to 7 locked angles across the theta sweep.
- Added a second RP corpus case with non-z-axis geometry and multi-phi sample locking to validate true azimuth-cut coverage.
- Corpus validation now also records external-reference deltas for RP pattern samples when `external_reference_candidate.pattern_samples` is present.
- Added `nec2c` external RP sample candidates for the multi-phi x-axis corpus case so parity tracking now covers both current RP decks.
- RP corpus cases can now opt into external-pattern CI gates via `ExternalGain_absolute_dB` and `ExternalAxialRatio_absolute` in `tolerance_gates`.
- Corpus validation now also supports optional external impedance CI gates (`ExternalR_*`/`ExternalX_*`) for scalar, multi-source, and frequency-sweep candidates.
- Enabled the first external impedance CI-gated case (`frequency-sweep-dipole`) with absolute candidate thresholds (`ExternalR_absolute_ohm=15.0`, `ExternalX_absolute_ohm=50.0`).
- Enabled a second external impedance CI-gated case (`dipole-ground-51seg`) with absolute candidate thresholds (`ExternalR_absolute_ohm=10.0`, `ExternalX_absolute_ohm=30.0`).
- Roadmap now defines a required benchmark-mode matrix across all target classes: CPU single-threaded, CPU multithreaded, and GPU offload.
- CLI now accepts `--exec <cpu|hybrid|gpu>` for real runs; `hybrid`/`gpu` are scaffolded execution modes that currently fall back to CPU with explicit diagnostics.
- `--exec hybrid` now performs coarse-grain multithreaded FR sweep solving (parallel per-frequency solve with ordered report output); GPU execution remains scaffolded.
- `--exec hybrid` now uses split-lane FR scheduling (CPU-parallel lane + GPU-candidate lane) with deterministic ordered report output; GPU-candidate lane points currently emit explicit fallback warnings and execute on CPU until GPU kernels are wired.
- Hybrid and GPU-mode fallback routing now flows through a concrete `nec_accel` dispatch API (`dispatch_frequency_point`) so future GPU kernel wiring has a stable integration seam.
- Added an opt-in accelerator stub dispatch path (`FNEC_ACCEL_STUB_GPU=1`) so `DispatchDecision::RunOnGpu` can be exercised end-to-end in CLI hybrid and gpu execution flows without changing output contracts.
- Added a tracked parity item for filename-steered 4nec2 solver-binary drop-in compatibility mode, including contract-preservation and throughput validation goals.
- Retargeted 4nec2 external-kernel drop-in compatibility work to a farther-future window (Phase 4-5) after assessing real NEC2MP replacement artifacts and integration scope.
- Expanded PAR-011 with an implementation discovery checklist (binary-name matrix, install/invocation contract, file side effects, dependency surface, fixtures, and benchmark protocol) to reduce future re-research cost.
- Added GNU NEC (`https://sourceforge.net/projects/gnu-nec/`) as an additional open-source reference candidate in architecture and PAR-011 source notes.
- Refined filename-steered 4nec2 compatibility warnings to explicitly report whether execution was auto-steered or an explicit `--exec` value was preserved.
- Extended drop-in compatibility contract tests to cover both `nec2dxs*` and `4nec2*` alias-name detection paths.
- Populated `docs/par011-dropin-evidence-memo.md` with concrete NEC2MP artifact evidence (inventory, readme findings, SHA256 fingerprints) and a phased docs-only PAR-011 implementation plan with `AT-PAR011-*` acceptance tests.
- Explicitly postponed PAR-011 compatibility harness-skeleton work in current scope (option 3 deferred).
- Explicitly postponed PAR-011 compatibility harness-skeleton work in current scope (option 3 deferred).
- **PH2-CHK-003 — LD/TL/NT implemented semantics (2026-05-10)**: LD cards (types 0–5) and TL lossless-line cards (`type=0`) are now parsed in `nec_parser` and applied as impedance stamps in the solver; NT cards are parsed for staged portability and emit a deferred-support warning instead of an unknown-card warning. 5 `ld_loads.rs` and 3 `tl_cards.rs` integration tests updated to Phase-2 assertions; 14 corpus reference entries in `reference-results.json` updated (3 LD loaded-value cases, 4 TL coupled-dipole cases, 7 NT deferred-warning cases); `parser_warnings.rs`, `report_contract.rs`, and `scriptability_contract.rs` tests updated to Phase-2 contracts.
- **PH2-CHK-007 — NEC-5 validation matrix ticked done (2026-04-30)**: The PH2-CHK-007 traceability matrix in `docs/corpus-validation-strategy.md` (row IDs `PH2N5-001` … `PH2N5-010`) carries explicit `in-scope implemented` / `in-scope deferred` / `out-of-scope` statuses with corpus case mappings, and `phase2_nec5_matrix_rows_are_traceable_to_corpus_cases` in `apps/nec-cli/tests/corpus_validation.rs` enforces row-ID presence, status validity, and corpus-case existence in CI. The PH2-CHK-007 done signal is therefore already met by prior PH2-CHK-005 work; this entry records the roadmap tick.
- **PH2-CHK-002 — Buried/near-ground guardrails ticked done (2026-04-30)**: `buried_wire_geometry_error` in `apps/nec-cli/src/geometry_validation.rs` fails fast with an actionable diagnostic when active-ground decks include `z<0` segments; `buried_wire_with_active_ground_fails_fast_with_actionable_error` and `near_ground_wire_with_active_ground_runs_without_deferred_warning` regression tests lock both branches in `apps/nec-cli/tests/ground_diagnostics.rs`; supported `dipole-gn2-near-ground-51seg` and unsupported `dipole-gn2-buried-unsupported` corpus fixtures are gated by warning / forbidden-warning / `expected_hallen_error_contains` contracts; `par002_ground_checklist_cases_are_present_and_contracted` enforces the matrix. The PH2-CHK-002 done signal is therefore already met by prior PH2-CHK-001 work; this entry records the roadmap tick.
- **PH2-CHK-004 — Report/table parity ticked done (2026-04-30)**: All 6 table sections implemented and CI-locked — `FEEDPOINTS`, `SOURCES`, `LOADS`, `CURRENTS`, `RADIATION_PATTERN`, `SWEEP_POINTS`; 5 report-contract tests in `apps/nec-cli/tests/report_contract.rs` lock headers, row parsing, section presence, and per-frequency block ordering (`FEEDPOINTS → SOURCES → LOADS → CURRENTS → SWEEP_POINTS`); 7 scriptability-contract tests in `apps/nec-cli/tests/scriptability_contract.rs` enforce machine-parseable stdout and stderr-only warnings. The PH2-CHK-004 done signal is already met by prior PH2-CHK-003 + 0.3.0 report work; this entry records the roadmap tick.
- **PH2-CHK-008 — Scriptability preservation ticked done (2026-04-30)**: 7 scriptability-contract tests lock stdout-only report stream, stderr-only warnings/bench records, `LOADS`-on-stdout (Phase-2), and exit-code contracts (code 1 on file-read error, code 2 on bad args); 11 core-flags-contract tests lock `--solver`, `--pulse-rhs`, `--exec`, `--bench-format` error/usage contracts and combined-flag success run. All 18 tests pass with zero regression after Phase-2 table and diagnostic additions; this entry records the roadmap tick.

## 0.2.0 — 2026-05-01

### Added

- **GM/GR card support**: GM (Geometry Move) and GR (Geometry Repeat) cards are now parsed and
  applied during geometry expansion. GM rotates/translates wire ranges (in-place or as copies with
  incremented tags); GR repeats all existing wires by successive z-axis rotations.
- **Segment current distribution table**: CLI output now includes a `CURRENTS` section listing
  TAG, SEG, I_RE, I_IM, I_MAG, I_PHASE (deg) for every segment after the feedpoint table.
- **Multi-wire Hallen fix**: per-wire homogeneous constants and endpoint constraints; passive wires
  now correctly receive zero RHS. Yagi and multi-source corpus validation now produces correct
  impedances (Yagi: 30.6+j5.0 Ω, multi-source: 152.4+j31.6 Ω each port).

### Changed

- GE I1=-1 warning now says "requests below-ground wire handling (no image method);
  treating as free-space" instead of a generic "not yet supported" message.
- GE I1=other unknown values now include the valid range hint
  `(valid values: 0=free-space, 1=PEC image, -1=below-ground)`.
- Updated corpus reference values for yagi-5elm-51seg and multi-source decks.

## 2026-04-24

### Added

- Added Phase 1 `GN` card support for perfect-ground (`GN 1`) Hallen runs.
- Added PEC image-method contribution path in Hallen matrix assembly.
- Added parser and solver tests that cover GN parsing and ground-aware matrix behavior.

### Changed

- Updated corpus ground regression reference (`dipole-ground-51seg`) to GN-aware Hallen values.
- Updated support boundary documentation to reflect current GN status (`GN 1` supported; Sommerfeld/Norton deferred).

## 2026-04-22

### Added

- Standard frontmatter requirements for all docs under `docs/`.
- Requirements, steering, roadmap, architecture, design, backlog, SBOM, and memory structure.
- CI automation design for docs stamping and validation.

### Changed

- Documented recent MoM kernel investigations and convergence behavior in new solver notes.
- Added an applied-math reference document with key EFIE/Pocklington/Hallen formulas.
- Added an implementation plan for continuity-enforcing rooftop/sinusoidal basis work.
- Added prominent README support/sponsoring note.
- Added project-local temporary work folder ignore guidance.
- Added regression tests for Hallén RHS symmetry/shape and Hallén/continuity solver behavior.
- Added CLI solver mode selection (`--solver hallen|pulse|continuity`) and single-chain continuity routing.
- Added documented mode benchmark deltas across segment counts in solver findings.
- Added explicit Hallen vs Pocklington matrix routing by solver mode and post-change benchmark notes.
- Added NEC2 reference-inspired pulse RHS wavelength normalization path:
  $$\\frac{1}{dl\\,\\lambda}$$
  and validation notes.

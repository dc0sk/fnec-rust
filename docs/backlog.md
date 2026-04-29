---
project: fnec-rust
doc: docs/backlog.md
status: living
last_updated: 2026-04-29
---

# Backlog

- [x] Implement `scripts/stamp-docs.sh` with `--from-git-diff` support.
- [x] Implement `scripts/validate-docs-frontmatter.sh` for strict checks.
- [x] Add `.github/workflows/docs-last-updated-pr.yml`.
- [x] Add `.github/workflows/docs-validate.yml`.
- [x] Add troubleshooting note for mobile approval-dialog limitations in contributor guidance.
- [ ] **Sinusoidal-basis EFIE (NEC2-style Pocklington fix)**: The current pulse/continuity solver modes use a pulse-basis Pocklington EFIE that is known to diverge from the physical solution for thin-wire antennas as the segment count increases. NEC2 uses sinusoidal (piecewise-sinusoidal) basis functions via `tbf`/`sbf`/`trio` which eliminate this divergence. Implementing the same sinusoidal-basis matrix assembly would make pulse/continuity modes accurate. Until then, these modes are marked experimental in the CLI. Reference: xnec2c `calculations.c`, NEC2 Theory of Operation (Burke & Poggio 1981).
  - Pulse RHS normalization candidate: $$\frac{1}{dl\,\lambda}$$
	- 2026-04-24 progress: scale-aware regularization added for projected continuity/sinusoidal solves in `crates/nec_solver/src/linear.rs`.
	- 2026-04-24 progress: CLI now emits explicit topology fallback warnings for continuity/sinusoidal non-single-chain cases, and sinusoidal residual fallback reuses one Hallen matrix assembly.
	- 2026-04-24 progress: CLI regression coverage added for continuity/sinusoidal non-single-chain topology fallback warnings and diagnostic mode labels (`apps/nec-cli/tests/topology_fallback.rs`).
	- 2026-04-24 progress: CLI regression coverage now also asserts sinusoidal residual-threshold fallback to Hallen (`diag: mode=sinusoidal->hallen(residual)`) on the reference dipole deck.
	- 2026-04-24 progress: CLI regression coverage now asserts experimental-warning mode gating (present for pulse/continuity/sinusoidal, absent for Hallen).
	- 2026-04-24 progress: extracted shared CLI diagnostics test helpers into `apps/nec-cli/tests/common/mod.rs` to centralize `diag:` mode parsing/assertion logic.
	- 2026-04-24 progress: CLI regression coverage now asserts `--pulse-rhs` contract in diagnostics (`diag: ... pulse_rhs=Raw|Nec2`) for pulse mode.
	- 2026-04-24 progress: CLI regression coverage now locks `diag: freq_mhz` to fixed six-decimal formatting (`14.200000`) on reference dipole runs.
	- 2026-04-24 progress: shared CLI diagnostics tests now assert `abs_res`/`rel_res` fields remain parseable finite non-negative numbers for reference Hallen and pulse runs.
	- 2026-04-24 progress: Raspberry Pi 5 target smoke-validated over SSH and automated with `scripts/pi-remote-workspace-check.sh` (sync + optional rustup bootstrap + remote workspace tests).
	- 2026-04-24 progress: added `scripts/pi-remote-benchmark.sh` to run repeatable Pi deck/solver timing sweeps and emit timestamped CSV baselines.
	- 2026-04-24 progress: added `scripts/pi-benchmark-compare.sh` to compute per deck/solver deltas between two benchmark CSV baselines (timing + residual ratios).
	- 2026-04-24 progress: `scripts/pi-benchmark-compare.sh` now supports threshold gating (`--max-delta-pct`) and mode-drift failure checks (`--fail-on-mode-drift`) for CI-style regression enforcement.
	- 2026-04-24 progress: added `.github/workflows/benchmark-compare.yml` to run benchmark delta gates in PRs when benchmark CSV inputs are present, with manual dispatch overrides.
	- 2026-04-24 progress: benchmark-compare workflow now publishes skip reasons and a compare-result preview in the Actions job summary for quick PR review.
	- 2026-04-25 progress: RP cards are now executed in the CLI report path with a `RADIATION_PATTERN` section (`THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO`), and regression coverage was added via `corpus/dipole-freesp-rp-51seg.nec` plus report-contract tests.
	- 2026-04-25 progress: `apps/nec-cli/tests/corpus_validation.rs` now parses `RADIATION_PATTERN` rows and tolerance-gates stored RP sample gains from `corpus/reference-results.json`.
	- 2026-04-25 progress: RP corpus validation now also gates `GAIN_V_DB`, `GAIN_H_DB`, and `AXIAL_RATIO` for stored sample angles, not only total `GAIN_DB`.
	- 2026-04-25 progress: RP corpus validation angle coverage increased from 2 sample angles to 7 (`0°, 30°, 60°, 90°, 120°, 150°, 180°` at `φ=0°`).
	- 2026-04-25 progress: added `corpus/dipole-xaxis-rp-grid-51seg.nec` to lock multi-phi RP behavior on an x-axis dipole with representative `(theta, phi)` samples.
	- 2026-04-25 progress: corpus validation now logs external-reference deltas for RP sample rows, and `dipole-freesp-rp-51seg` carries a first `nec2c` pattern candidate for parity tracking.
	- 2026-04-25 progress: `dipole-xaxis-rp-grid-51seg` now also carries `nec2c` external RP samples, so the observational parity path covers both current RP corpus decks.
	- 2026-04-25 progress: RP corpus cases can now promote those external pattern candidates into CI gates with optional `ExternalGain_absolute_dB` / `ExternalAxialRatio_absolute` thresholds.
	- 2026-04-25 progress: corpus validation now also supports optional external impedance gates (`ExternalR_absolute_ohm`, `ExternalX_absolute_ohm`, `ExternalR_percent_rel`, `ExternalX_percent_rel`) for scalar, source, and FR candidate deltas.
	- 2026-04-25 progress: `frequency-sweep-dipole` now enables the first external impedance candidate gates with `ExternalR_absolute_ohm=15.0` and `ExternalX_absolute_ohm=50.0`.
	- 2026-04-26 progress: `dipole-ground-51seg` now also enables external impedance candidate gates with `ExternalR_absolute_ohm=10.0` and `ExternalX_absolute_ohm=30.0`.
	- 2026-04-26 progress: roadmap now explicitly requires CPU single-threaded, CPU multithreaded, and GPU benchmark modes across all target classes.
	- 2026-04-26 progress: CLI execution-mode plumbing landed with `--exec <cpu|hybrid|gpu>` and diag `exec=...`; initial scaffold path exposed real-application mode selection before hybrid runtime work.
	- 2026-04-26 progress: `--exec hybrid` now runs coarse-grain multithreaded FR sweeps (parallel per-frequency solves with ordered output), while `--exec gpu` remains CPU fallback scaffolding.
	- 2026-04-26 progress: hybrid GPU-candidate lane routing now calls `nec_accel::dispatch_frequency_point(...)` before CPU fallback, establishing the first concrete accelerator integration seam.
	- 2026-04-26 progress: `DispatchDecision::RunOnGpu` is now handled non-fatally in CLI hybrid and gpu execution flows via an accelerator stub branch (`FNEC_ACCEL_STUB_GPU=1`) that preserves report/diag contracts while using CPU emulation.
	- 2026-04-26 progress: first concrete GPU kernel scaffold landed (`nec_accel::gpu_kernels::HallenFrGpuKernel`) with Hallen far-field radiation pattern computation stubs. Module provides GPU-compatible data layouts and API surface for future CUDA/OpenCL kernel implementations, complete with unit tests (8) and integration test suite (6) validating dipole patterns, multi-segment arrays, azimuth symmetry, and numerical edge cases.
	- 2026-04-27 progress: native CLI startup now auto-selects execution mode when `--exec` is omitted by running a quick startup probe and choosing the most suitable available mode (`cpu`/`hybrid`/`gpu`) for current frequency sweep shape and dispatch availability.
	- 2026-04-29 progress: tightened external impedance gates for 5 corpus cases (`dipole-ground-51seg` R 10→8, `yagi-5elm-51seg` R/X 30/70→25/55, `tl-two-dipoles-linked` R/X 5/20→4/16, `frequency-sweep-dipole` R/X 15/50→14/47, `multi-source` R/X 15/50→12/40) based on actual fnec-vs-nec2c deltas plus 10–15% headroom.
	- 2026-04-29 progress: tightened external RP gain gates for `dipole-freesp-rp-51seg` (0.1→0.08 dB, actual max |dGain|=0.068) and `dipole-xaxis-rp-grid-51seg` (0.1→0.04 dB, actual max |dGain|=0.034), matching the actual-delta-plus-headroom pattern used for impedance gates.
	- 2026-04-29 progress: implemented PEC ground-aware RP far-field in `nec_solver::farfield` (image contribution + upper-hemisphere normalization + below-horizon null contract), wired CLI RP path through `ground` model, and added corpus regression fixture `corpus/dipole-ground-rp-51seg.nec` with reference pattern samples.
	- 2026-04-29 progress: activated GN type 0 simple finite-ground model in Hallen matrix assembly using a complex Fresnel-style reflection factor from EPSE/SIG, replaced the prior deferred GN0 warning path, and added corpus regression fixture `corpus/dipole-gn0-fresnel-51seg.nec`.
	- 2026-04-29 progress: added CLI scriptability regression tests (`apps/nec-cli/tests/scriptability_contract.rs`) to lock stable machine-parseable report headers on stdout and enforce warning/output stream separation (warnings on stderr, report-only stdout).
	- 2026-04-29 progress: added CLI core-flags contract tests (`apps/nec-cli/tests/core_flags_contract.rs`) covering parse-error contracts (missing/invalid values, unknown options, missing deck path) and a full core-flag success invocation path.
	- 2026-04-29 progress: extended scriptability contracts to bench mode by locking that `bench_json:` records remain on stderr while stdout keeps the stable report stream for machine parsers.
	- 2026-04-29 progress: hardened scriptability contracts with three additional locks: bench CSV records remain on stderr (`bench_csv:` prefix absent from stdout), nonexistent deck exits with code 1 and "cannot read" on stderr (no report on stdout), and no-arg invocation exits with code 2 and usage on stderr (no report on stdout).
	- 2026-04-29 progress: completed Phase 1 core-flag CLI contract coverage by locking the remaining parser branches in `apps/nec-cli/tests/core_flags_contract.rs` (invalid `--solver`, missing/invalid `--pulse-rhs`, missing/invalid `--bench-format`, missing/invalid `--ex3-i4-mode`, and unexpected extra argument handling).
	- 2026-04-29 progress: extended CLI report output for multi-frequency FR runs with a machine-parseable `SWEEP_POINTS` summary section (`N_POINTS`, `FREQ_MHZ TAG SEG Z_RE Z_IM`) and locked the format via `apps/nec-cli/tests/report_contract.rs`.
	- 2026-04-29 progress: added corpus case `dipole-loaded-noncollinear-hallen` (with `--allow-noncollinear-hallen`) to lock the experimental non-collinear Hallen path on the top-hat loaded deck, and added checklist test `phase1_loaded_corpus_gap_cases_are_present_and_contracted` to keep both loaded-case contracts (`dipole-loaded` blocked path + experimental opt-in path) present in CI.
	- 2026-04-29 progress: closed the Phase 1 CLI scriptability/batch-friendliness checklist item using the expanded contract suite (`apps/nec-cli/tests/scriptability_contract.rs` and `apps/nec-cli/tests/core_flags_contract.rs`) that locks stable stdout report headers, stderr-only warnings/bench records (`bench_json:`/`bench_csv:`), deterministic parse-error exit codes, and no-arg/missing-deck stream behavior.
	- 2026-04-29 progress: strengthened loaded-case parity tracking by adding external impedance gates to `dipole-loaded-noncollinear-hallen` (`ExternalR_absolute_ohm=5`, `ExternalX_absolute_ohm=35`) and extending `phase1_loaded_corpus_gap_cases_are_present_and_contracted` to require those external gates.
	- 2026-04-29 progress: added an EZNEC-informed concrete Phase 2 implementation checklist to `docs/roadmap.md` (`PH2-CHK-001..008`), explicitly mapping roadmap parity IDs (`PRT-*`, `CP-*`) to implementation targets and validation artifacts (test/corpus files).
	- 2026-04-29 progress: started PH2-CHK-005 corpus truth expansion for current/phase classes by adding deterministic `current_samples` and current/phase tolerance gates to `dipole-freesp-51seg` in `corpus/reference-results.json`, plus checklist gate `phase2_current_phase_corpus_contract_is_present_and_contracted` in `apps/nec-cli/tests/corpus_validation.rs`.
	- 2026-04-29 progress: started PH2-CHK-006 geometry diagnostics slice by adding an early-fail CLI guard for unsupported intersecting wire segments (non-endpoint crossings) before solve, with deterministic integration contracts in `apps/nec-cli/tests/geometry_diagnostics.rs` covering both failing crossing-wire geometry and allowed endpoint junctions.

## Parity-driven backlog items

- [x] **PAR-001 / 4nec2-EZNEC text-report parity contract / Owner: CLI+Reporting / Target: Phase 1 / Issue: #14**
	Resolution: PAR-001 v1 contract implemented and CI-gated on 2026-04-23 (`FORMAT_VERSION 1`, deterministic headers/table, report contract integration test).
	Follow-up scope (gain/pattern/current report breadth and richer parity expectations) remains tracked under Phase 1-2 roadmap/report parity items.

- [ ] **PAR-002 / Advanced ground parity plan / Owner: Solver / Target: Phase 2 / Issue: #15**
	Resolution criteria: NEC-4-class ground scope document published; Sommerfeld validation corpus added; tolerance pass documented for supported near-ground cases.
	- 2026-04-28 progress: started PAR-002 docs-first discovery slice by adding a scoped finite-ground plan to `docs/nec4-support.md` (scope/non-goals/completion evidence).
	- 2026-04-28 progress: added PAR-002 finite-ground validation workflow and closure checklist to `docs/corpus-validation-strategy.md` to define capture/gating expectations before solver expansion.
	- 2026-04-28 progress: added GN type 2 (Sommerfeld/Norton) deferred-ground corpus fixture `corpus/dipole-gn2-deferred.nec` with warning contract and free-space fallback regression gate in `corpus/reference-results.json`.
	- 2026-04-28 progress: added GN type 2 and GN type 3 warning-contract regression tests to `apps/nec-cli/tests/ground_diagnostics.rs`, extending deferred-ground test coverage beyond the existing GN type 0 test.
	- 2026-04-28 progress: added `par002_ground_checklist_cases_are_present_and_contracted` gate test to `apps/nec-cli/tests/corpus_validation.rs` to lock both the PEC and deferred-ground corpus fixtures in CI.
	- 2026-04-28 progress: extended `nec_model::card::GnCard` with `eps_r: Option<f64>` and `sigma: Option<f64>` fields; updated parser to read EPSE/SIG medium-parameter fields from GN cards (e.g. `GN 2 0 0 0 13.0 0.005`).
	- 2026-04-28 progress: updated `nec_solver::geometry::GroundModel::Deferred` to carry `eps_r` and `sigma` from the parsed GN card; updated `ground_model_from_deck()` to pass them through.
	- 2026-04-28 progress: updated CLI `warn_deferred_ground_model()` to append parsed medium parameters to the deferred-ground warning (e.g. `[parsed: EPSE=13, SIG=0.005 S/m]`) when present.
	- 2026-04-28 progress: added parser tests `gn_card_with_medium_params_parses_eps_r_and_sigma` and `gn_card_without_medium_params_uses_none`; added geometry test `ground_model_carries_medium_params_from_gn_card`; added CLI integration test `gn_type2_warning_includes_parsed_medium_params`.
	- 2026-04-28 progress: fixed GN -1 (null ground) to map to `GroundModel::FreeSpace` without emitting a deferred-ground warning; added geometry unit test `ground_model_gn_negative1_returns_free_space`, CLI integration test `gn_negative1_null_ground_is_silent_free_space`, corpus fixture `corpus/dipole-gn-1-null.nec`, and `corpus/reference-results.json` entry with forbidden-warning contract; extended PAR-002 checklist gate to include the GN -1 case with `expect_forbidden_contract`.
	- 2026-04-29 progress: implemented the first PH2-CHK-001 GN2 runtime slice by mapping `GN 2` to the active scoped finite-ground path (same approximation family as current GN0) instead of deferred free-space fallback; `GN 3` remains deferred.
	- 2026-04-29 progress: updated GN2 contracts across solver/CLI/corpus (`crates/nec_solver/src/geometry.rs`, `apps/nec-cli/tests/ground_diagnostics.rs`, `apps/nec-cli/tests/corpus_validation.rs`, `corpus/dipole-gn2-deferred.nec`, `corpus/reference-results.json`) to require no deferred GN2 warning and to gate regression impedance on the in-scope above-ground GN2 class.

- [x] **PAR-003 / Mainstream NEC workflow card coverage / Owner: Parser+Solver / Target: Phase 2 / Issue: #16**
	Resolution criteria: load/source/TL-network card subset listed as supported in `docs/nec4-support.md`; integration tests added per card family; deck portability checklist passes for selected reference decks.
	Resolution: Completed 2026-04-28. PAR-003 staged mainstream-card subset is documented in `docs/nec4-support.md`, integration coverage exists across card families (parser/solver/CLI/corpus), and portability checklist gate `par003_portability_checklist_cases_are_present_and_contracted` is passing.
	- 2026-04-27 progress: LD load-family coverage expanded with LD type 1 (parallel RLC) solve support in `nec_solver::build_loads`, plus CLI integration regression (`apps/nec-cli/tests/ld_loads.rs`) and new corpus fixture `corpus/dipole-ld-loaded-51seg.nec`.
	- 2026-04-27 progress: LD load-family coverage now also includes LD types 2 (series RL) and 3 (series RC) in `nec_solver::build_loads`, with CLI regression coverage in `apps/nec-cli/tests/ld_loads.rs`.
	- 2026-04-27 progress: added corpus regression cases `dipole-ld-series-rl-51seg` and `dipole-ld-series-rc-51seg` to lock LD types 2/3 behavior in CI.
	- 2026-04-27 progress: TL cards now have an executable initial subset in solver runtime (`type=0`, `NSEG=0` or `1`) via impedance-matrix stamps (`nec_solver::build_tl_stamps`), with CLI regression coverage (`apps/nec-cli/tests/tl_cards.rs` and `apps/nec-cli/tests/parser_warnings.rs`).
	- 2026-04-27 progress: TL endpoint `segment=0` now maps to the tag center segment with an explicit runtime warning (instead of being rejected), improving mainstream deck portability.
	- 2026-04-27 progress: added corpus regression case `tl-two-dipoles-linked-seg0` (`corpus/tl-two-dipoles-linked-seg0.nec`) to lock TL segment=0 center-mapping behavior in CI.
	- 2026-04-27 progress: TL segment=0 mapping now resolves even-segment tags deterministically to the lower center segment, with explicit warning text and solver regression coverage in `nec_solver::tl` tests.
	- 2026-04-27 progress: added corpus regression case `tl-two-dipoles-linked-nseg0` (`corpus/tl-two-dipoles-linked-nseg0.nec`) to lock TL `NSEG=0` single-section shorthand behavior in CI.
	- 2026-04-27 progress: added corpus regression case `tl-two-dipoles-linked-seg0-even52` (`corpus/tl-two-dipoles-linked-seg0-even52.nec`) to lock TL segment=0 lower-center mapping on even-segment tags in CI.
	- 2026-04-27 progress: EX type 3 is now accepted in the excitation path (currently treated as EX type 0 semantics), unblocking mainstream deck portability while full normalization semantics remain pending.
	- 2026-04-27 progress: added CLI warning-contract regression `ex_type3_runs_without_unsupported_error` in `apps/nec-cli/tests/parser_warnings.rs` to lock EX type 3 acceptance behavior.
	- 2026-04-27 progress: added corpus regression case `dipole-ex3-freesp-51seg` (`corpus/dipole-ex3-freesp-51seg.nec`) to lock EX type 3 runtime acceptance in CI.
	- 2026-04-27 progress: added CLI parity regression `ex_type3_matches_ex_type0_feedpoint_impedance` in `apps/nec-cli/tests/ex_cards.rs` to lock current EX type 3 == EX type 0 electrical behavior.
	- 2026-04-27 progress: added solver-level parity regression `ex_type3_matches_ex_type0_vector` in `crates/nec_solver/src/excitation.rs` to lock EX RHS equivalence between type 3 and type 0.
	- 2026-04-27 progress: EX type 3 with non-default `I4` now emits an explicit runtime warning in CLI (`warn_ex_type3_normalization_semantics`), with regression coverage in `apps/nec-cli/tests/parser_warnings.rs`.
	- 2026-04-27 progress: corpus warning-contract coverage added for EX type 3 non-default `I4` via `dipole-ex3-i4-freesp-51seg` and `expected_warning_substrings` checks in `apps/nec-cli/tests/corpus_validation.rs`.
	- 2026-04-27 progress: started EX type 3 normalization semantics branch with a non-breaking solver scaffold `Ex3NormalizationMode` in `crates/nec_solver/src/excitation.rs`; default behavior remains legacy (type 3 == type 0), and provisional `I4` divisor semantics are currently test-only.
	- 2026-04-28 progress: wired runtime CLI flag `--ex3-i4-mode <legacy|divide-by-i4>` through solver and Hallen RHS paths; default remains legacy while `divide-by-i4` enables experimental EX3 I4-divisor semantics with explicit warning-contract coverage.
	- 2026-04-28 progress: EX type 1 is now accepted in the parser/solver path as a staged portability fallback; current runtime behavior treats it like EX type 0 and emits an explicit warning that full current-source semantics are pending.
	- 2026-04-28 progress: added CLI warning-contract regression `ex_type1_runs_with_portability_warning_without_unsupported_error` and parity regression `ex_type1_matches_ex_type0_feedpoint_impedance` to lock current EX type 1 behavior.
	- 2026-04-28 progress: added corpus regression case `dipole-ex1-freesp-51seg` with expected warning lock to keep EX type 1 deck portability covered in CI.
	- 2026-04-29 progress: implemented the first EX type 1 semantics slice for `--solver pulse` via explicit current-constraint rows in the pulse solve path; added pulse-specific warning/parity tests and corpus case `dipole-ex1-pulse-current-freesp-51seg` that forbids the legacy portability warning.
	- 2026-04-28 progress: EX type 2 is now accepted in the parser/solver path as a staged portability fallback; current runtime behavior treats it like EX type 0 and emits an explicit warning that incident-plane-wave semantics are pending.
	- 2026-04-28 progress: added CLI warning-contract regression `ex_type2_runs_with_portability_warning_without_unsupported_error` and parity regression `ex_type2_matches_ex_type0_feedpoint_impedance` to lock current EX type 2 behavior.
	- 2026-04-28 progress: added corpus regression case `dipole-ex2-freesp-51seg` with expected warning lock to keep EX type 2 deck portability covered in CI.
	- 2026-04-28 progress: EX type 4 is now accepted in the parser/solver path as a staged portability fallback; current runtime behavior treats it like EX type 0 and emits an explicit warning that segment-current semantics are pending.
	- 2026-04-28 progress: added CLI warning-contract regression `ex_type4_runs_with_portability_warning_without_unsupported_error` and parity regression `ex_type4_matches_ex_type0_feedpoint_impedance` to lock current EX type 4 behavior.
	- 2026-04-28 progress: added corpus regression case `dipole-ex4-freesp-51seg` with expected warning lock to keep EX type 4 deck portability covered in CI.
	- 2026-04-28 progress: EX type 5 is now accepted in the parser/solver path as a staged portability fallback; current runtime behavior treats it like EX type 0 and emits an explicit warning that qdsrc semantics are pending.
	- 2026-04-28 progress: added CLI warning-contract regression `ex_type5_runs_with_portability_warning_without_unsupported_error` and parity regression `ex_type5_matches_ex_type0_feedpoint_impedance` to lock current EX type 5 behavior.
	- 2026-04-28 progress: added corpus regression case `dipole-ex5-freesp-51seg` with expected warning lock to keep EX type 5 deck portability covered in CI.
	- 2026-04-28 progress: PT cards are now parsed for staged portability (instead of being treated as unknown cards), and runtime emits an explicit deferred-support warning while PT electrical semantics remain ignored.
	- 2026-04-28 progress: added CLI warning-contract regression `pt_card_emits_deferred_warning_but_run_succeeds` and corpus regression case `dipole-pt-freesp-51seg` to lock PT staged portability behavior in CI.
	- 2026-04-28 progress: NT cards are now parsed for staged portability (instead of being treated as unknown cards), and runtime emits an explicit deferred-support warning while NT electrical semantics remain ignored.
	- 2026-04-28 progress: added CLI warning-contract regression `nt_card_emits_deferred_warning_but_run_succeeds` and corpus regression case `dipole-nt-freesp-51seg` to lock NT staged portability behavior in CI.
	- 2026-04-28 progress: added combined PT+NT warning-contract regression `pt_and_nt_cards_emit_deferred_warnings_and_run_succeeds` and corpus case `dipole-pt-nt-freesp-51seg` to lock multi-card staged portability behavior in CI.
	- 2026-04-28 progress: strengthened parser-level PT/NT regressions in `crates/nec_parser/src/lib.rs` to assert exact `raw_fields` preservation, locking staged portability token capture semantics in CI.
	- 2026-04-28 progress: added CLI warning-contract regression `repeated_pt_and_nt_cards_emit_deduplicated_warnings_per_family` to lock current deduplicated warning behavior for repeated PT/NT cards.
	- 2026-04-28 progress: added corpus regression case `dipole-pt-nt-repeated-freesp-51seg` to lock repeated PT/NT deck portability and warning contract in corpus validation CI.
	- 2026-04-28 progress: external validation path expanded: EZNEC via Wine is now available for future external impedance candidate capture alongside nec2c seeds.
	- 2026-04-28 progress: updated `docs/corpus-validation-strategy.md` to document EZNEC-via-Wine as a fallback capture path in external-reference workflow guidance.
	- 2026-04-28 progress: added parser regression `repeated_pt_and_nt_cards_preserve_order_and_raw_fields` in `crates/nec_parser/src/lib.rs` to lock repeated PT/NT token preservation order.
	- 2026-04-28 progress: added parser regression `repeated_nt_and_pt_cards_preserve_order_and_raw_fields` in `crates/nec_parser/src/lib.rs` to lock repeated NT/PT token preservation order.
	- 2026-04-28 progress: added parser regression `interleaved_pt_nt_cards_preserve_card_sequence` in `crates/nec_parser/src/lib.rs` to lock interleaved PT/NT sequence preservation with exact raw fields.
	- 2026-04-28 progress: added CLI warning-contract regression `interleaved_pt_and_nt_cards_emit_deduplicated_warnings_per_family` and corpus case `dipole-pt-nt-interleaved-freesp-51seg` to lock interleaved PT/NT deferred-warning portability behavior in CI.
	- 2026-04-28 progress: added parser regression `interleaved_nt_pt_cards_preserve_card_sequence`, CLI warning-contract regression `interleaved_nt_and_pt_cards_emit_deduplicated_warnings_per_family`, and corpus case `dipole-nt-pt-interleaved-freesp-51seg` to lock NT-first interleaved PT/NT portability behavior in CI.
	- 2026-04-28 progress: corpus validation harness now supports per-case `cli_args`, and `dipole-ex3-i4-divide-by-i4-freesp-51seg` was added to lock EX3 `--ex3-i4-mode divide-by-i4` warning semantics in corpus CI.
	- 2026-04-28 progress: added corpus case `dipole-ex3-i4-two-divide-by-i4-freesp-51seg` (deck `dipole-ex3-i4-two-freesp-51seg.nec`) to lock EX3 `I4=2` divide-by-i4 warning semantics in corpus CI.
	- 2026-04-28 progress: added corpus case `dipole-ex3-i4-two-freesp-51seg` (legacy mode) to lock EX3 `I4=2` non-default warning semantics on the default runtime path.
	- 2026-04-28 progress: corpus validation now supports `forbidden_warning_substrings`, and EX3 divide-by-i4 corpus cases now assert the legacy non-default-I4 warning is absent when divide-by-i4 mode is selected.
	- 2026-04-28 progress: corpus validation now supports `expected_warning_counts`; repeated/interleaved PT/NT and EX3 legacy/divide-by-i4 corpus cases now assert exact warning occurrence counts for stronger warning-contract parity.
	- 2026-04-28 progress: expanded `expected_warning_counts` corpus locks to single-card EX1/EX2/EX4/EX5 and PT/NT portability cases, enforcing one-warning-per-case contracts across those PAR-003 fixtures.
	- 2026-04-28 progress: expanded `forbidden_warning_substrings` corpus locks across EX1/EX2/EX4/EX5 and PT/NT (single/combo/repeated/interleaved) cases to assert no fallback unsupported/unknown-card warnings leak into staged-portability runs.
	- 2026-04-28 progress: added EX3 `I4=0` corpus mode-matrix locks: legacy `dipole-ex3-freesp-51seg` now forbids non-default/experimental warnings, and `dipole-ex3-divide-by-i4-freesp-51seg` locks divide-by-i4 experimental-warning behavior with forbidden legacy-nondefault warning.
	- 2026-04-28 progress: refreshed `docs/nec4-support.md` support declarations to match implemented PAR-003 subset (staged EX1/2/4/5, EX3 mode path, TL subset, PT/NT staged handling, LD subset).
	- 2026-04-28 progress: added PAR-003 portability checklist gate `par003_portability_checklist_cases_are_present_and_contracted` in `apps/nec-cli/tests/corpus_validation.rs` to lock selected deck presence and warning-contract coverage.
	- 2026-04-28 progress: added CLI warning-contract regression `nt_then_pt_cards_emit_deferred_warnings_and_run_succeeds` to lock PT/NT deferred-warning behavior independent of card order.
	- 2026-04-28 progress: added corpus regression case `dipole-nt-pt-freesp-51seg` to lock NT-then-PT card-order portability and warning contract in corpus validation CI.
	- 2026-04-28 progress: added CLI warning-contract regression `repeated_nt_and_pt_cards_emit_deduplicated_warnings_per_family` and corpus case `dipole-nt-pt-repeated-freesp-51seg` to lock deduplicated deferred warnings for repeated reversed-order NT/PT cards.
	- 2026-04-28 progress: enabled external impedance candidate gates for `tl-two-dipoles-linked` with conservative thresholds (`ExternalR_absolute_ohm=5.0`, `ExternalX_absolute_ohm=20.0`) as external-impedance gate seed-2.
	- 2026-04-28 progress: enabled external impedance candidate gates for `multi-source` with conservative thresholds (`ExternalR_absolute_ohm=15.0`, `ExternalX_absolute_ohm=50.0`) as external-impedance gate seed-3.
	- 2026-04-28 progress: enabled external impedance candidate gates for `yagi-5elm-51seg` with conservative thresholds (`ExternalR_absolute_ohm=30.0`, `ExternalX_absolute_ohm=70.0`) as external-impedance gate seed-5.
	- 2026-04-29 progress: tightened external impedance gates across 5 corpus cases based on actual solver-vs-nec2c deltas (10-15% headroom): `dipole-ground-51seg` R 10→8; `yagi-5elm-51seg` R/X 30/70→25/55; `tl-two-dipoles-linked` R/X 5/20→4/16; `frequency-sweep-dipole` R/X 15/50→14/47; `multi-source` R/X 15/50→12/40. `dipole-ld-loaded-51seg` unchanged (large known gap).
	- 2026-04-27 progress: added corpus validation case `tl-two-dipoles-linked` (`corpus/tl-two-dipoles-linked.nec`) to lock TL subset behavior in CI, with a first `nec2c` external impedance candidate captured for parity tracking.

- [ ] **PAR-004 / xnec2c-style workbench parity / Owner: GUI+CLI / Target: Phase 3 / Issue: #17**
	Resolution criteria: usability acceptance checklist defined and demonstrated (interactive sweep inspection, graphical result browsing, fast edit-run-inspect loop); at least one end-to-end demo captured.

- [ ] **PAR-005 / AutoEZ-class automation primitives / Owner: Automation / Target: Phase 3 / Issue: #18**
	Resolution criteria: variable sweep runner, resonance targeting helper, convergence study helper, and matching-network helper are implemented with CLI entry points and documented examples.

- [ ] **PAR-006 / necpp-style embeddability and diagnostics / Owner: Core APIs / Target: Phase 3 / Issue: #19**
	Resolution criteria: stable automation API surface documented; binding strategy decision recorded; geometry diagnostics catch at least the known invalid/fragile model classes with actionable errors.

- [ ] **PAR-007 / AutoEZ procurement gate / Owner: Product / Target: Phase 3 start / Issue: #20**
	Resolution criteria: go/no-go decision recorded with evidence from open-tool and documentation benchmarking; if go, purchase and benchmark plan logged; if no-go, defer rationale and next review date logged.

- [x] **PAR-008 / NEC-5 validation-manual coverage matrix / Owner: Solver+Validation / Target: Phase 2 / Issue: #21**
	Resolution: Completed 2026-04-26 for coverage-matrix scope. NEC-5 Validation Manual scenario classes are mapped to current fnec-rust in-scope equivalents; mapped in-scope classes have reproducible corpus tests with explicit tolerance gating; known out-of-scope classes are documented with rationale and phase deferral. Matrix source: `docs/corpus-validation-strategy.md` section "NEC-5 validation coverage matrix (PAR-008)".

- [ ] **PAR-009 / xnec2c-optimize external optimizer-loop parity / Owner: Automation+CLI / Target: Phase 3 / Issue: #22**
	Resolution criteria: deterministic objective-evaluation CLI/API contract documented; at least one xnec2c-optimize-style optimization flow reproduced end-to-end with fnec-rust automation hooks; machine-readable outputs verified stable across repeated runs.

- [ ] **PAR-010 / Distributed authenticated cluster execution mode / Owner: Core+Automation / Target: Phase 5+ (after full GPU support) / Issue: #23**
	Resolution criteria: architecture decision doc approved (auth model, trust boundary, transport, failure semantics); authenticated node discovery implemented with capability cache; work-content/result cache implemented with deterministic cache keys and invalidation policy; SSH-backed bootstrap flow documented and demonstrated on at least 2 worker nodes.
	- Sequencing constraint: implementation starts only after full GPU solver support (matrix fill + solve path) is complete and benchmarked.

- [ ] **PAR-011 / 4nec2 solver-binary drop-in compatibility mode / Owner: CLI+Runtime / Target: Phase 4-5 / Issue: #24**
	Resolution criteria: filename-steered compatibility profile detects known 4nec2 kernel binary names, preserves expected invocation/report contracts for drop-in operation, and demonstrates multithreaded kernel replacement throughput gains against single-thread external baseline.
	- 2026-04-26 assessment: deferred from Phase 2-3 to Phase 4-5 after reviewing real NEC2MP replacement artifacts (`nec2dxs500/1K5/3k0/5k0/8k0/11k` variants plus external install procedure docs). Full drop-in parity likely requires Windows-specific replacement semantics, binary-name matrix handling, and compatibility validation against external tool expectations beyond current CLI contract scope.
	- 2026-04-26 progress: populated `docs/par011-dropin-evidence-memo.md` with concrete artifact fingerprints/readme findings and added a phased docs-only implementation plan with acceptance tests (`AT-PAR011-*`).
	- 2026-04-26 scope decision: compatibility harness skeleton work is postponed for now (option 3 deferred).
	- Discovery checklist (capture before implementation starts):
		- Binary-name matrix: confirm exact accepted executable names/casing and segment-limit mapping (`nec2dxs500.exe`, `nec2dxs1K5.exe`, `nec2dxs3k0.exe`, `nec2dxs5k0.exe`, `nec2dxs8k0.exe`, `nec2dxs11k.exe`).
		- Install contract: document required replacement/copy steps in Windows 4nec2 installation paths and whether side-by-side binary variants are expected.
		- Invocation contract: capture argv shape, working-directory expectations, stdin/stdout/stderr behavior, exit-code semantics, and timeout/error handling expected by 4nec2.
		- File side effects: enumerate all expected temporary/input/output files and lifecycle rules (create/overwrite/delete) during external-kernel execution.
		- Dependency surface: verify companion DLL/runtime requirements (if any) and loader-path assumptions in both portable and installed setups.
		- Compatibility fixtures: archive representative external-kernel call traces and outputs for each binary variant as regression fixtures.
		- Benchmark method: define throughput comparison protocol against the legacy single-thread kernel on identical decks, with per-variant segment-count bands.
		- Reference sources: index `nec2mp-readme.pdf` notes, the cited URL (`http://users.otenet.gr/~jmsp`), and GNU NEC SourceForge project notes (`https://sourceforge.net/projects/gnu-nec/`) into the evidence memo (`docs/par011-dropin-evidence-memo.md`) for future PAR-011 kickoff.

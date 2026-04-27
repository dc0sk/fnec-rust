---
project: fnec-rust
doc: docs/backlog.md
status: living
last_updated: 2026-04-24
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

## Parity-driven backlog items

- [x] **PAR-001 / 4nec2-EZNEC text-report parity contract / Owner: CLI+Reporting / Target: Phase 1 / Issue: #14**
	Resolution: PAR-001 v1 contract implemented and CI-gated on 2026-04-23 (`FORMAT_VERSION 1`, deterministic headers/table, report contract integration test).
	Follow-up scope (gain/pattern/current report breadth and richer parity expectations) remains tracked under Phase 1-2 roadmap/report parity items.

- [ ] **PAR-002 / Advanced ground parity plan / Owner: Solver / Target: Phase 2 / Issue: #15**
	Resolution criteria: NEC-4-class ground scope document published; Sommerfeld validation corpus added; tolerance pass documented for supported near-ground cases.

- [ ] **PAR-003 / Mainstream NEC workflow card coverage / Owner: Parser+Solver / Target: Phase 2 / Issue: #16**
	Resolution criteria: load/source/TL-network card subset listed as supported in `docs/nec4-support.md`; integration tests added per card family; deck portability checklist passes for selected reference decks.

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

- [ ] **PAR-010 / Distributed authenticated cluster execution mode / Owner: Core+Automation / Target: Phase 4-5 / Issue: #23**
	Resolution criteria: architecture decision doc approved (auth model, trust boundary, transport, failure semantics); authenticated node discovery implemented with capability cache; work-content/result cache implemented with deterministic cache keys and invalidation policy; SSH-backed bootstrap flow documented and demonstrated on at least 2 worker nodes.

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

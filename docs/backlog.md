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

- [ ] **PAR-008 / NEC-5 validation-manual coverage matrix / Owner: Solver+Validation / Target: Phase 2 / Issue: #21**
	Resolution criteria: NEC-5 Validation Manual scenario classes mapped to fnec-rust in-scope equivalents; each mapped class has at least one reproducible corpus test with explicit tolerance gating; known out-of-scope classes are documented with rationale. Matrix source: `docs/corpus-validation-strategy.md` section "NEC-5 validation coverage matrix (PAR-008)".

- [ ] **PAR-009 / xnec2c-optimize external optimizer-loop parity / Owner: Automation+CLI / Target: Phase 3 / Issue: #22**
	Resolution criteria: deterministic objective-evaluation CLI/API contract documented; at least one xnec2c-optimize-style optimization flow reproduced end-to-end with fnec-rust automation hooks; machine-readable outputs verified stable across repeated runs.

- [ ] **PAR-010 / Distributed authenticated cluster execution mode / Owner: Core+Automation / Target: Phase 4-5 / Issue: #23**
	Resolution criteria: architecture decision doc approved (auth model, trust boundary, transport, failure semantics); authenticated node discovery implemented with capability cache; work-content/result cache implemented with deterministic cache keys and invalidation policy; SSH-backed bootstrap flow documented and demonstrated on at least 2 worker nodes.

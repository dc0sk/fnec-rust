---
project: fnec-rust
doc: docs/pr-summary-feat-external-impedance-gate-seed-2.md
status: living
last_updated: 2026-04-26
---

# PR Summary: feat/external-impedance-gate-seed-2

## Branch goal

Deliver the execution-mode and external-parity groundwork while preserving report contracts and test stability:

- optional external impedance/parity gating enablement in corpus flow
- CLI execution modes (`cpu`, `hybrid`, `gpu`) with explicit diagnostics
- hybrid sweep runtime progression from scaffold to real split-lane execution
- accelerator dispatch seam and stub execution policy integration
- deferred-but-tracked drop-in compatibility roadmap with early filename-steered scaffold

## Major delivered changes

1. Hybrid/GPU execution progression
- Added CLI execution-mode scaffold and diagnostics.
- Added coarse-grain hybrid FR sweep execution with deterministic ordered output.
- Added split-lane hybrid scheduling (CPU lane + GPU-candidate lane).
- Centralized accelerator execution policy in `nec_accel`.
- Added stub dispatch path (`FNEC_ACCEL_STUB_GPU=1`) and non-fatal RunOnGpu handling.

2. Compatibility scaffolding and contract hardening
- Added filename-steered drop-in compatibility profile for `nec2dxs*` and `4nec2*` names.
- Added contract tests for alias-based steering and explicit `--exec` override behavior.
- Refined compatibility warnings to distinguish:
  - auto-steered default mode
  - preserved explicit `--exec`

3. Validation and reference-planning updates
- Extended docs/backlog/roadmap/changelog for deferred drop-in parity scope.
- Added reference candidates and PAR-011 discovery checklist.
- Added collaboration efficiency guide for rate-limit-aware workflow.

## Test evidence

Latest branch verification:

- `cargo fmt --all --check` passed
- `cargo check` passed
- `cargo test` passed (workspace-wide)
- `cargo test -p nec-cli --test exec_modes -- --nocapture` passed

## Risk notes

- `PAR-011` full 4nec2 drop-in parity remains intentionally deferred (Phase 4-5) due to Windows replacement workflow/invocation-compat scope.
- Current drop-in implementation is a scaffold and should not be marketed as full external-kernel compatibility yet.
- Accelerator RunOnGpu path currently uses CPU emulation under stub policy until real kernels are wired.

## Merge readiness checklist

- [x] Branch clean after tests
- [x] Formatting and compile checks pass
- [x] Workspace tests pass
- [x] CLI compatibility tests pass
- [x] Documentation updated for delivered behavior and deferred scope

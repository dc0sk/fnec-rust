---
project: fnec-rust
doc: docs/ph7-chk-005-real-gpu-benchmark.md
status: living
last_updated: 2026-06-27
---

# PH7-CHK-005: real discrete-GPU benchmark evidence + crossover

## Requirement / change

Roadmap checklist `PH7-CHK-005` (Phase 7): record real discrete-GPU benchmark
evidence (not the CI software rasterizer) for the RP and Z-matrix-fill kernels on
at least one vendor, publish it, and document the problem-size crossover where the
GPU path beats the CPU. Done signal: a real-GPU benchmark series exists; the
crossover problem size is documented; `docs/benchmarks.md` frontmatter gate passes.

## Approach

A reproducible harness, `apps/nec-cli/examples/gpu_crossover.rs`, measures the two
production GPU kernels against their CPU equivalents across problem sizes on real
hardware:

- **Z-matrix fill — kernel-only.** GPU dispatch via `microbench_zmatrix_dispatch`
  (PH7-CHK-002), which excludes the one-time wgpu device-init; CPU via
  `nec_solver::assemble_z_matrix`. Swept over segment counts 32…1536.
- **RP far-field — production wall-clock.** GPU via the production
  `run_rp_farfield_batch_wgpu` (re-acquires the device, so device-init is
  *included*); CPU via `nec_solver::compute_radiation_pattern`. Swept over
  observation-point counts 181…16201.

Run with `cargo run --release -p nec-cli --example gpu_crossover`; human tables go
to stderr, a machine-readable JSON artifact to stdout.

## Evidence (vendor: AMD)

- **Adapter**: `AMD Radeon Graphics (RADV RENOIR)` — Vulkan backend, integrated GPU.
- **Artifact**: `benchmarks/real-gpu-crossover.json` (representative run, committed).
- **Documented results + crossover**: `docs/benchmarks.md` §"Real discrete-GPU
  crossover (PH7-CHK-005)".

### Crossover

- **Z-matrix fill (kernel-only)**: GPU beats CPU below 32 segments — the smallest
  size measured — and the lead grows to ~240× by 1,536 segments (CPU ≈ O(N²); GPU
  dispatch ≈ flat). The only reason a *single* small solve does not benefit is the
  one-time device-init (~25 ms here), which a frequency sweep or the GPU-resident
  solve (PH7-CHK-003) amortizes.
- **RP far-field (production wall-clock, device-init included)**: GPU is ~1.5–1.8×
  faster than CPU across 181…16201 points, the lead widening with point count.

## Notes

- The `microbench` (PH7-CHK-002) separates device-init from dispatch, which is
  what lets the Z-fill crossover be stated for the *kernel* rather than the
  device-init-dominated single-shot wall-clock.
- This run also refreshes the stale `FNEC_ACCEL_STUB_GPU` references in
  `docs/benchmarks.md` (that env var was retired in PH7-CHK-001) and serves as the
  AMD-vendor real-hardware record that complements `docs/multi-vendor-gpu.md`.

## Test results

`cargo test --workspace` clean; `cargo clippy --workspace` clean; the harness
runs end-to-end on the AMD target and emits the committed artifact; the
`docs/benchmarks.md` frontmatter gate passes.

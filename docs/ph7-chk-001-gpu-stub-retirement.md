---
project: fnec-rust
doc: docs/ph7-chk-001-gpu-stub-retirement.md
status: living
last_updated: 2026-06-27
---

# PH7-CHK-001: Retire the GPU CPU-emulation scaffold

## Requirement / change

Roadmap checklist `PH7-CHK-001` (see `docs/roadmap.md`, Phase 7):

> Retire or realize the `nec_accel::gpu_kernels` CPU-emulation scaffold. Either
> replace `HallenFrGpuKernel`/`compute_hallen_fr_*_stub` with real wgpu dispatch,
> or delete the scaffold and route the hybrid-dispatch decision
> (`dispatch_frequency_point`) through `wgpu_device`. **No code path may report
> CPU compute time as GPU time.**
>
> Done signal: no remaining "stub: CPU computation marked as GPU" path;
> `cargo test -p nec_accel` clean; `ExecutionPath::GpuStubEmulation` either
> removed or backed by real device work.

## Design decision: retire (not realize)

We **retire** the scaffold rather than realize it, because the real GPU path
already exists and is wired:

- `nec_accel::wgpu_device` contains real WGSL kernels (`run_rp_farfield_wgpu`,
  `run_rp_farfield_batch_wgpu`, `fill_zmatrix_wgpu`), and `--exec gpu` already
  dispatches the radiation-pattern and Z-matrix-fill work through them
  (`apps/nec-cli/src/solve_session.rs`).
- The `gpu_kernels` compute functions are valuable **only as the CPU parity
  reference** for those wgpu kernels — not as a production GPU path.
- Genuine per-frequency GPU dispatch through the `dispatch_frequency_point`
  seam (so a hybrid sweep runs some frequency points on the GPU) is real future
  work, tracked as **PH7-CHK-004**. Until that lands, the honest state of the
  hybrid GPU-candidate lane is "always CPU fallback".

So "realizing" the scaffold here would either duplicate the existing wgpu
kernels or pre-empt PH7-CHK-004. Retiring removes the dishonest labelling now
and leaves a clean seam for PH7-CHK-004 to fill.

## What the lie was

Three concrete places reported CPU compute as GPU:

1. **`ExecutionPath::GpuStubEmulation` + `FNEC_ACCEL_STUB_GPU`** — setting the
   env var forced `dispatch_frequency_point` to return `RunOnGpu`, after which
   `execute_frequency_point` ran the CPU closure but tagged it
   `GpuStubEmulation`, and the CLI emitted *"dispatched to accelerator stub
   backend; solving with CPU emulation"*.
2. **`--gpu-fr` flag** — a production CLI flag that routed radiation-pattern
   computation through `compute_hallen_fr_batch_stub` (pure CPU) and presented
   the result as GPU-accelerated. Superseded by `--exec gpu` (real wgpu).
3. **Naming / docs** — `compute_hallen_fr_*_stub`, the module header ("Invoke
   GPU kernel (stub: CPU computation marked as GPU)"), the `KernelTiming.exec_us`
   field ("GPU kernel execution time ... stub: CPU compute time"), and two dead
   data-prep structs (`HallenRhsGpuKernel`, `PocklingtonMatrixGpuKernel`) that
   were never instantiated.

## Changes

### `crates/nec_accel`

- **lib.rs**: remove `ExecutionPath`, `execute_frequency_point`,
  `FNEC_ACCEL_STUB_GPU`/`stub_gpu_enabled`. `dispatch_frequency_point` now
  always returns `FallbackToCpu` with reason *"per-frequency GPU dispatch not
  yet wired (PH7-CHK-004)"*. `DispatchDecision { RunOnGpu, FallbackToCpu }` is
  kept — `RunOnGpu` is reserved for PH7-CHK-004. Crate-doc status table rewritten
  to call `gpu_kernels` the **CPU reference**, not a "stub".
- **gpu_kernels.rs**: reframed as the CPU reference far-field implementation and
  the shared GPU-ready data layouts (`GpuSegment`, `GpuFarFieldPoint`) consumed
  by the wgpu kernels. `compute_hallen_fr_point_stub` → `compute_hallen_fr_point_cpu`,
  `compute_hallen_fr_batch_stub` → `compute_hallen_fr_batch_cpu`. `KernelTiming`
  documented honestly as a CPU timing breakdown. Dead `HallenRhsGpuKernel` and
  `PocklingtonMatrixGpuKernel` removed.

### `apps/nec-cli`

- **cli_args.rs / main.rs / solve_session.rs**: `--gpu-fr` flag and the
  `enable_gpu_fr` CPU-stub radiation-pattern branch removed. The hybrid sweep
  path no longer uses `execute_frequency_point`/`ExecutionPath`; GPU-candidate
  lane points are run on CPU and reported honestly via the existing
  *"...GPU-candidate lane, but ... running those points on CPU fallback"*
  warning. The `gpu_stub_count` / "accelerator stub backend" warning is removed.
- **warnings.rs**: `warn_execution_mode_fallback(Gpu)` drops the dead
  `RunOnGpu` arm; the default `--exec gpu requested ... using CPU solve path`
  warning is unchanged (still accurate for the dense linear solve, which stays
  on CPU until PH7-CHK-003).

## Tests

- `crates/nec_accel/src/lib.rs` unit tests: dropped the env-hack / stub-emulation
  tests; kept honest `dispatch_frequency_point` → `FallbackToCpu` coverage.
- `apps/nec-cli/tests/exec_modes.rs`: the two `*_accepts_accelerator_stub_dispatch_path`
  tests rewritten to assert the honest behaviour (hybrid → CPU-fallback warning,
  GPU → no "accelerator stub"/"CPU emulation" text).
- `apps/nec-cli/tests/hallen_fr_gpu_stub.rs` → `hallen_fr_cpu_reference.rs`,
  using the renamed `_cpu` functions.
- `apps/nec-cli/tests/core_flags_contract.rs`: `--gpu-fr` removed from the
  all-flags success test.

## Test results (2026-06-27)

- `cargo test -p nec_accel --features wgpu` — clean: 25 unit tests (incl. the
  renamed `wgpu_rp_farfield_parity_vs_cpu_reference`) + `gpu_hallen_solve` +
  `gpu_zmatrix_parity` parity tests pass.
- `cargo test --workspace` — clean: 533 tests pass across all suites,
  including the rewritten `exec_modes` fallback tests and the renamed
  `hallen_fr_cpu_reference` integration tests.
- `cargo clippy --workspace` — clean (no warnings).
- Grep audit: no remaining `GpuStubEmulation`, `FNEC_ACCEL_STUB_GPU`,
  `--gpu-fr`, `compute_hallen_fr_*_stub`, `HallenRhsGpuKernel`,
  `PocklingtonMatrixGpuKernel`, or "CPU computation marked as GPU" in
  non-test source.

---
project: fnec-rust
doc: docs/gpu-arch.md
status: living
last_updated: 2026-05-03
---

# GPU Architecture Decision

This document records the GPU acceleration architecture decisions for fnec-rust
Phase 5.  It covers: the target API matrix, the first-offload candidate, the
real-hardware validation minimum before matrix-fill/solve kernels land, and the
rationale for each choice.

This document satisfies **GAP-007** (GPU rollout criteria).

---

## Contents

1. [Decision summary](#1-decision-summary)
2. [Target API matrix](#2-target-api-matrix)
3. [Rationale: why wgpu](#3-rationale-why-wgpu)
4. [First-offload candidate](#4-first-offload-candidate)
5. [Milestone gate sequence](#5-milestone-gate-sequence)
6. [Real-hardware validation minimum](#6-real-hardware-validation-minimum)
7. [Fallback and CPU-parity contract](#7-fallback-and-cpu-parity-contract)
8. [Stub-to-real migration path](#8-stub-to-real-migration-path)
9. [Out of scope for Phase 5](#9-out-of-scope-for-phase-5)

---

## 1. Decision summary

| Question | Decision |
|:---------|:---------|
| Primary GPU API | **wgpu** (Rust-native, Vulkan/Metal/DX12/OpenCL/WGSL backend) |
| Compute shader language | **WGSL** (portable; compiles to SPIR-V for Vulkan, MSL for Metal, HLSL for DX12) |
| First-offload candidate | **RP far-field gain computation** (embarrassingly parallel, existing CPU reference in `nec_accel::gpu_kernels`) |
| CUDA support | Not in Phase 5 — CUDA is single-vendor; wgpu covers NVIDIA via Vulkan |
| OpenCL support | Not as a first-class backend; wgpu's OpenCL adapter covers it where available |
| ROCm/AMD support | Via wgpu Vulkan backend on supported AMD GPUs |
| CPU fallback | Always present; GPU path is an opt-in acceleration, never a hard requirement |
| Numerical parity gate | GPU RP results must match CPU within RP tolerance (≤ 0.5 dB gain, ≤ 0.05 absolute axial ratio) before matrix-fill work begins |

---

## 2. Target API matrix

| Backend | API layer | Platforms | Phase 5 status |
|:--------|:----------|:----------|:---------------|
| Vulkan | wgpu (primary) | Linux (`x86_64`, `aarch64`), Windows 10+ | **First target** |
| Metal | wgpu | macOS 10.14+, iOS | Covered by wgpu; validated opportunistically |
| DX12 | wgpu | Windows 10+ | Covered by wgpu; validated opportunistically |
| OpenCL | wgpu OpenCL adapter | Linux (various), fallback on systems without Vulkan | Adapter coverage; not a primary gate |
| CUDA | — | NVIDIA (Linux/Windows) | **Not in Phase 5** — NVIDIA hardware covered via Vulkan |
| CPU software rasterizer | wgpu `dx12` or `wgpu_hal` soft | Any | Dev/CI fallback; not for production use |

The Raspberry Pi 5 target (aarch64, VideoCore VII / Vulkan 1.2) is a primary
validation target alongside the `x86_64` workstation.

---

## 3. Rationale: why wgpu

| Factor | wgpu | OpenCL C directly | CUDA directly |
|:-------|:-----|:------------------|:--------------|
| Rust integration | Native (`wgpu` crate, `wgpu-hal`) | `opencl3` / `ocl` crates, C-FFI | `cust` / bindgen, NVIDIA-only |
| Portability | Vulkan + Metal + DX12 + OpenCL in one API | OpenCL only | NVIDIA-only |
| Shader language | WGSL (portable, typed, auditable) | OpenCL C / SPIR-V | CUDA C++ |
| CI without GPU | Software rasterizer (`wgpu` in headless mode) | No standard soft path | No |
| License compatibility | MIT/Apache 2.0 | Mixed | Proprietary |
| Maintenance trajectory | Active, backed by gfx-rs / WebGPU standard | Stable but declining | Proprietary |

**Key constraint**: `cargo deny` policy requires permissive-licensed deps.
wgpu (MIT + Apache 2.0) and WGSL are clean.  CUDA's proprietary toolkit would
require a policy exception.

**CI without GPU hardware**: wgpu can run compute shaders against a software
rasterizer (e.g. `lavapipe` on Linux or wgpu's `dx12` software adapter on
Windows).  This enables GPU-path unit and integration tests in CI without
needing GPU runners, which is critical for the corpus tolerance gate.

---

## 4. First-offload candidate

**RP (radiation pattern) far-field gain computation.**

### Why RP first

1. **Embarrassingly parallel**: each `(theta, phi)` observation point is
   independent — no inter-thread data dependencies.  Perfect GPU workload shape.

2. **Existing stub baseline**: `nec_accel::gpu_kernels::HallenFrGpuKernel` and
   `compute_hallen_fr_batch_stub` already implement the CPU reference path with
   the exact data layout (`GpuSegment`, `GpuFarFieldPoint`) that a WGSL compute
   shader will consume.  The migration path from stub to real GPU is a shader
   implementation, not an API redesign.

3. **Bounded numerical risk**: RP gain tolerances (≤ 0.5 dB) are looser than
   impedance tolerances (≤ 0.1 % relative).  A first GPU kernel that produces
   correct gain values is a lower-risk starting point than a matrix-fill kernel
   where errors propagate into impedance.

4. **Measurable speedup on large RP grids**: a full hemisphere `(theta: 5°
   steps, phi: 5° steps)` is 1296 observation points per frequency, each
   requiring a sum over all segments.  For `yagi-5elm-51seg` (51 segments per
   wire × 5 elements), the GPU parallelism advantage is meaningful even on a
   mid-range consumer GPU.

### What "first offload" means in practice

The first wgpu kernel will:

1. Accept the same `Vec<GpuSegment>` + `Vec<Complex64>` inputs as the stub.
2. Upload to GPU buffer; dispatch a WGSL compute shader.
3. Readback `Vec<GpuFarFieldPoint>`.
4. Be gated by the numerical parity test (see §6).

The `compute_hallen_fr_batch_stub` path remains as fallback when no Vulkan
adapter is available.

---

## 5. Milestone gate sequence

Phase 5 GPU work proceeds through these gates in order.  **No gate may be
skipped.**

| Gate | Description | Criterion |
|:-----|:------------|:----------|
| **G1** Architecture locked | This document merged and reviewed | `docs/gpu-arch.md` in `main` |
| **G2** wgpu scaffold | `wgpu` added to `nec_accel/Cargo.toml`; device enumeration compiles and runs in CI (software adapter) | `cargo test -p nec_accel` passes with wgpu present |
| **G3** RP WGSL kernel | First real WGSL compute shader for RP gain; numerical parity test passes | RP results match CPU stub within tolerance on `dipole-freesp-rp-51seg`; CI green |
| **G4** CLI integration | `--exec gpu` routes RP computation through wgpu kernel when adapter available | Integration test: `FNEC_FORCE_WGPU=1 fnec --exec gpu dipole-freesp-rp-51seg.nec` produces RP output within tolerance |
| **G5** CPU-vs-GPU benchmark | Benchmark both paths on RP-heavy deck; assert GPU ≥ 0.8× CPU on large RP grid (regression: GPU cannot be >20% slower than CPU) | Benchmark gate added to `scripts/pi-benchmark-compare.sh` |
| **G6** Matrix-fill prototype | First WGSL kernel for Hallen Z-matrix fill on reference geometry | Numerical parity: filled Z-matrix elements match CPU within 1e-6 relative on `dipole-freesp-51seg` |
| **G7** Full GPU solver path | GPU matrix-fill + GPU LU solve (or CPU LU on GPU-filled matrix) for complete Hallen solve | Impedance within tolerance gates on all corpus decks; `cargo test` clean |

Phase 5 ends when **G7** is complete and the CI benchmarking dashboard (§ next
Phase 5 checklist batch) is operational.

---

## 6. Real-hardware validation minimum

**Before matrix-fill/solve kernels (G6+) land in `main`**, the following
real-hardware validation must be complete:

### Minimum hardware set

| # | Target | GPU | Requirement |
|:-:|:-------|:----|:------------|
| 1 | local-workstation (`x86_64` Linux) | Any Vulkan 1.1+ GPU | RP kernel (G3) passes tolerance gate |
| 2 | target-pi5 (`aarch64` Linux) | VideoCore VII (Vulkan 1.2) | RP kernel (G3) passes tolerance gate |

Both targets are already in the benchmark matrix (`docs/benchmarks.md`).

**Rationale**: a GPU kernel that passes in CI (software rasterizer) but fails
on real hardware is a regression in disguise.  The Pi5 target is included
because VideoCore VII is a distinct GPU architecture (tile-based deferred
rasterization) and may expose floating-point precision differences not visible
on desktop discrete GPUs.

### Tolerance gates for RP kernel validation

Sourced from `docs/requirements.md` RP tolerance row:

| Metric | Tolerance |
|:-------|:----------|
| `GAIN_DB` (total gain) | ≤ 0.5 dB absolute vs CPU reference |
| `GAIN_V_DB`, `GAIN_H_DB` | ≤ 0.5 dB absolute vs CPU reference |
| `AXIAL_RATIO` | ≤ 0.05 absolute vs CPU reference |

These are the same gates already applied in `apps/nec-cli/tests/corpus_validation.rs`
for the RP corpus cases.  The GPU kernel must pass the identical test.

### Tolerance gates for matrix-fill kernel validation (G6)

| Metric | Tolerance |
|:-------|:----------|
| Z-matrix element (real/imag) | ≤ 1 × 10⁻⁶ relative vs CPU reference |
| Feedpoint impedance (R, X) | Within the standard corpus impedance tolerance (≤ 0.1 % relative or ≤ 0.05 Ω absolute) |

---

## 7. Fallback and CPU-parity contract

The GPU path is **always optional**.  The following contract applies at every
gate:

1. If no Vulkan-capable adapter is available, `fnec` falls back to CPU
   automatically with a diagnostic on stderr:
   ```
   info: no GPU adapter available — using CPU path
   ```

2. If `--exec cpu` is specified, the GPU path is never attempted.

3. The JSON output contract (`--output-format json`), the text report contract,
   and all corpus tolerance gates must be satisfied identically by both the CPU
   and GPU paths.

4. `cargo test` must pass in CI (no GPU hardware) without any `#[ignore]`
   attributes on GPU-path tests — the software rasterizer covers this.

5. `FNEC_ACCEL_STUB_GPU=1` remains as a regression escape hatch for the CPU-
   emulation path; it is distinct from the real wgpu path.

---

## 8. Stub-to-real migration path

The existing CPU-emulation stubs in `nec_accel::gpu_kernels` are the migration
starting point, not dead code:

| Existing stub | Migration target |
|:--------------|:----------------|
| `compute_hallen_fr_batch_stub` | Replaced by wgpu compute dispatch in G3 |
| `HallenFrGpuKernel::execute_stub` | Replaced by `HallenFrGpuKernel::execute_wgpu` in G3 |
| `HallenRhsGpuKernel` stub | Replaced by WGSL RHS assembly kernel in G6 |
| `PocklingtonMatrixGpuKernel` stub | Replaced by WGSL Pocklington kernel (post-G7, if pulse mode GPU is in scope) |

The stub variants remain as named `_stub` functions until the corresponding
real kernel has passed all validation gates, at which point the stub is either
removed or retained as a `cfg(test)` reference implementation.

---

## 9. Out of scope for Phase 5

The following are explicitly deferred:

| Deferred item | Reason |
|:--------------|:-------|
| CUDA-native kernels | Single-vendor; covered by Vulkan on NVIDIA; NVIDIA users are not blocked |
| ROCm-native (HIP) kernels | AMD hardware covered by Vulkan via wgpu; HIP requires a separate toolchain |
| Distributed/cluster execution | Explicitly gated behind full GPU solver support (Phase 5 roadmap note) |
| NEC-5-class surface / mixed-potential | Separate architecture decision required; not part of GPU acceleration path |
| Windows / macOS primary CI | Opportunistic validation only; Linux `x86_64` and `aarch64` are primary |

---

## See also

- [`docs/phase5-entry-criteria.md`](phase5-entry-criteria.md) — Phase 5 entry gate (all criteria met)
- [`docs/benchmarks.md`](benchmarks.md) — CPU baseline timing and benchmark methodology
- [`docs/requirements.md`](requirements.md) — Numerical tolerance matrix
- [`crates/nec_accel/src/gpu_kernels.rs`](../crates/nec_accel/src/gpu_kernels.rs) — Existing GPU kernel stubs (migration starting point)

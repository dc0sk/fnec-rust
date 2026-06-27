---
project: fnec-rust
doc: docs/multi-vendor-gpu.md
status: living
last_updated: 2026-06-27
---

# Multi-Vendor GPU Backend Matrix

## Purpose

This document records the validated GPU backend matrix for fnec-rust's wgpu
acceleration layer.  Validation was carried out in two rounds: PH6-CHK-004
(AMD Renoir RADV, initial) and PH6-CHK-004 extended (Intel ANV, Nvidia
proprietary, Broadcom V3DV on aarch64).  The deferred ROCm/SYCL path is
tracked as DEC-008.

---

## Hardware Surveyed

Four GPU backends across three physical machines were validated.

### Machine 1 — Local workstation (AMD Renoir / Manjaro)

| Field | Value |
|:------|:------|
| PCI address | `07:00.0` |
| Device name | AMD/ATI Renoir [Radeon Vega Series / Radeon Vega Mobile Series] |
| PCI vendor / device ID | `0x1002` / `0x1636` |
| Form factor | Integrated GPU (Ryzen 5000-series APU) |
| Vulkan driver | RADV (Mesa open-source) — `libvulkan_radeon.so` |
| Vulkan API version | 1.4.335 |
| ICD manifest | `/usr/share/vulkan/icd.d/radeon_icd.json` |
| DRM render node | `/dev/dri/renderD128` |
| Kernel / OS | 6.18.18-1-MANJARO (Manjaro Linux) |

wgpu 0.19.4 adapter enumeration output on this machine:

```
adapter[0]: name="AMD Radeon Graphics (RADV RENOIR)"
            backend=Vulkan  device_type=IntegratedGpu
            vendor=0x1002   device=0x1636   driver="radv"

adapter[1]: name="AMD Radeon Graphics (radeonsi, renoir, ACO, DRM 3.64, 6.18.18-1-MANJARO)"
            backend=Gl      device_type=Other
            vendor=0x1002   device=0x0000   driver=""
```

wgpu selects adapter[0] (Vulkan / RADV) as the preferred compute backend on this
hardware.  The OpenGL adapter[1] is available as a secondary fallback but is not
used for compute.

---

### Machine 2 — ThinkPad T480 (Intel UHD 620 + Nvidia GeForce MX150 / Ubuntu)

| Field | Value |
|:------|:------|
| CPU GPU | Intel UHD Graphics 620 (KBL GT2), PCI `00:02.0`, vendor `0x8086`, device `0x5917` |
| Discrete GPU | Nvidia GeForce MX150 (GP108M), PCI `01:00.0`, vendor `0x10de`, device `0x1d10` |
| Intel Vulkan driver | ANV (Mesa open-source) — `intel_icd.json` |
| Nvidia Vulkan driver | Proprietary — `nvidia_icd.json` |
| DRM render nodes | `/dev/dri/renderD128` (Intel), `/dev/dri/renderD129` (Nvidia) |
| Kernel / OS | 7.0.0-14-generic x86_64 (Ubuntu) |

wgpu 0.19.4 adapter enumeration on this machine:

```
adapter[0]: name="Intel(R) UHD Graphics 620 (KBL GT2)"
            backend=Vulkan  device_type=IntegratedGpu
            vendor=0x8086   device=0x5917   driver="Intel open-source Mesa driver"

adapter[1]: name="NVIDIA GeForce MX150"
            backend=Vulkan  device_type=DiscreteGpu
            vendor=0x10de   device=0x1d10   driver="NVIDIA"

adapter[2]: name="llvmpipe (LLVM 21.1.8, 256 bits)"
            backend=Vulkan  device_type=Cpu
            vendor=0x10005  device=0x0000   driver="llvmpipe"

adapter[3]: name="NVIDIA GeForce MX150/PCIe/SSE2"
            backend=Gl      device_type=Other
            vendor=0x10de   device=0x0000   driver=""
```

---

### Machine 3 — Raspberry Pi 5 (VideoCore VII / V3DV / Raspberry Pi OS aarch64)

| Field | Value |
|:------|:------|
| SoC GPU | VideoCore VII, Broadcom vendor `0x14e4`, device `0x55701c33` |
| Vulkan driver | V3DV (Mesa open-source) — `broadcom_icd.json` |
| ICD manifest | `/usr/share/vulkan/icd.d/broadcom_icd.json` |
| DRM render node | `/dev/dri/renderD128` |
| Model | Raspberry Pi 5 Model B Rev 1.1 |
| Kernel / OS | 6.12.75+rpt-rpi-2712 aarch64 (Raspberry Pi OS) |

wgpu 0.19.4 adapter enumeration on this machine:

```
adapter[0]: name="V3D 7.1.10.2"
            backend=Vulkan  device_type=IntegratedGpu
            vendor=0x14e4   device=0x55701c33  driver="V3DV Mesa"

adapter[1]: name="llvmpipe (LLVM 19.1.7, 128 bits)"
            backend=Vulkan  device_type=Cpu
            vendor=0x10005  device=0x0000   driver="llvmpipe"

adapter[2]: name="V3D 7.1.10.2"
            backend=Gl      device_type=IntegratedGpu
            vendor=0x14e4   device=0x0000   driver=""
```

---

## Validation Results

All test runs used `cargo test -p nec_accel --features wgpu`.
No workarounds or backend-specific flags were required on any machine.

### AMD Renoir RADV (Machine 1 — Manjaro)

| Test | Status | Notes |
|:-----|:-------|:------|
| `wgpu_enumerate_adapters_does_not_panic` | ✓ pass | RADV RENOIR enumerated as adapter[0] |
| `wgpu_noop_compute_pipeline_succeeds_or_skips_gracefully` | ✓ pass | `NoOpPipelineResult::Success` |
| `wgpu_rp_farfield_parity_vs_cpu_reference` | ✓ pass | wgpu kernel matches CPU stub within 1 % |
| `gpu_zmatrix_fill_matches_cpu_within_1e4_relative` | ✓ pass | Z-matrix dispatch path clean |
| `gpu_hallen_path_feedpoint_impedance_within_2_ohm_of_cpu` | ✓ pass | Hallén solver dispatch path clean |

### Intel UHD 620 ANV + Nvidia GeForce MX150 (Machine 2 — Ubuntu)

`cargo test -p nec_accel --features wgpu` — 17 tests, 0 failures.

| Test | Status | Notes |
|:-----|:-------|:------|
| `wgpu_enumerate_adapters_does_not_panic` | ✓ pass | Intel ANV adapter[0], Nvidia adapter[1] both enumerated |
| `wgpu_noop_compute_pipeline_succeeds_or_skips_gracefully` | ✓ pass | `NoOpPipelineResult::Success` |
| `wgpu_rp_farfield_parity_vs_cpu_reference` | ✓ pass | Matches CPU stub within 1 % |
| `gpu_zmatrix_fill_matches_cpu_within_1e4_relative` | ✓ pass | Z-matrix path clean |
| `gpu_hallen_path_feedpoint_impedance_within_2_ohm_of_cpu` | ✓ pass | Hallén path clean |
| All other 12 tests | ✓ pass | No Intel- or Nvidia-specific deltas observed |

### Raspberry Pi 5 VideoCore VII V3DV (Machine 3 — aarch64)

`cargo test -p nec_accel --features wgpu` — 17 tests, 0 failures.

| Test | Status | Notes |
|:-----|:-------|:------|
| `wgpu_enumerate_adapters_does_not_panic` | ✓ pass | V3D 7.1.10.2 enumerated as adapter[0] |
| `wgpu_noop_compute_pipeline_succeeds_or_skips_gracefully` | ✓ pass | `NoOpPipelineResult::Success` |
| `wgpu_rp_farfield_parity_vs_cpu_reference` | ✓ pass | Matches CPU stub within 1 % |
| `gpu_zmatrix_fill_matches_cpu_within_1e4_relative` | ✓ pass | Z-matrix path clean |
| `gpu_hallen_path_feedpoint_impedance_within_2_ohm_of_cpu` | ✓ pass | Hallén path clean |
| All other 12 tests | ✓ pass | No V3DV- or aarch64-specific deltas observed |

**Dispatch note (updated 2026-06-27)**: the per-frequency `dispatch_frequency_point`
seam still returns `FallbackToCpu` (that lane is tracked separately), but the GPU
kernels themselves are now really dispatched to the device: `--exec gpu` runs the
RP far-field and Z-matrix-fill WGSL kernels, and — since PH7-CHK-003 — the
GPU-resident Hallén solve (fill → LU → refinement entirely on the GPU). On this
AMD Renoir/RADV target PH7-CHK-005 measured those real kernels (Z-fill ~200×
faster than CPU up to 1536 segments). The validated path is therefore: adapter
enumeration → device/queue creation → real RP / Z-fill / resident-solve dispatch.

No vendor-specific deltas or workarounds were required on any of the four tested backends.

---

## Backend Support Matrix

| Backend | Platform | Status | Notes |
|:--------|:---------|:-------|:------|
| Vulkan | Linux (AMD RADV) | ✓ Validated | PH6-CHK-004; RADV 1.4.335 on Renoir |
| Vulkan | Linux (Intel ANV) | ✓ Validated | T480, Intel UHD 620 KBL GT2; 17/17 tests pass |
| Vulkan | Linux (Nvidia proprietary) | ✓ Validated | T480, GeForce MX150 GP108M; 17/17 tests pass |
| Vulkan | Linux (Broadcom V3DV) | ✓ Validated | Raspberry Pi 5 VideoCore VII; 17/17 tests pass (aarch64) |
| Vulkan | Windows | Not tested | Expected to work; DX12 preferred on Windows |
| DX12 | Windows | Not tested | wgpu 0.19 supports DX12; no test hardware available |
| Metal | macOS | Not tested | wgpu 0.19 supports Metal; no test hardware available |
| OpenGL | Linux (Mesa) | Available | adapter[1] on Renoir; not used for compute |
| WebGPU | Browser | Not targeted | fnec-rust is a CLI/TUI/GUI desktop tool |
| OpenCL | Linux | Deferred | DEC-008; dated deferral PH7-CHK-006 (2026-06-27) |
| ROCm | Linux (AMD) | Deferred | DEC-008; dated deferral PH7-CHK-006 — Renoir APU outside ROCm support matrix |
| SYCL | Cross-platform | Deferred | DEC-008; dated deferral PH7-CHK-006 — no SYCL toolchain on target |

---

## ROCm / SYCL / OpenCL Deferred Path (DEC-008)

Staged expansion to AMD ROCm, OpenCL via wgpu extras, and SYCL is tracked as
DEC-008 and deferred past the current Phase 6 milestone sequence.

**Rationale for deferral**:

1. **RADV Vulkan already provides full coverage on AMD** — ROCm adds no new
   correctness guarantee for wire-antenna MoM workloads at the current problem
   sizes (≤1000 segments), and Vulkan is already validated above.

2. **ROCm toolchain complexity** — ROCm requires the `rocm` package stack
   (`rocm-hip-sdk`, `hip-runtime-amd`, `comgr`), which is substantially heavier
   than a Mesa Vulkan ICD and is not available in default CI images.

3. **wgpu 0.19 does not expose a native ROCm/HIP backend** — ROCm would require
   either wgpu's `opengl` feature + Mesa's Clover/rusticl OpenCL stack, or a
   separate `ash`/HIP FFI crate.  Neither path offers a benefit over RADV Vulkan
   for the current compute kernels (WGSL is backend-agnostic and compiles to
   SPIR-V for both Vulkan and, with a translation layer, ROCm).

4. **SYCL** is outside the Rust ecosystem; bridging it would require a C++ shim
   crate and a nightly toolchain. Deferred indefinitely.

**Trigger to re-evaluate DEC-008**: if a future compute kernel is bottlenecked on
Vulkan pipeline overhead (expected only at >10,000 segments with batch dispatch)
or if a CI runner with ROCm becomes available.

### PH7-CHK-006 outcome (2026-06-27): dated deferral

PH7-CHK-006 asked to *validate a native non-wgpu backend (ROCm or SYCL) on an AMD
target for the matrix-fill kernel, or record an explicit dated "not yet" with
rationale and the concrete blocking gaps.* **Outcome: deferred** — native ROCm/
SYCL validation is **not attempted** for the reasons below, each verified on the
AMD target on 2026-06-27.

**Concrete blocking gaps (verified, not asserted):**

1. **Hardware is outside AMD's ROCm support matrix.** The AMD target is a
   **Renoir** integrated APU (PCI device `0x1636`, `gfx90c`). AMD's official ROCm
   support matrix covers CDNA datacenter parts and a subset of discrete RDNA
   cards; Renoir/APU integrated GPUs are **not supported**. Running ROCm on it
   requires unsupported `HSA_OVERRIDE_GFX_VERSION` overrides, which would make any
   "validation" non-representative of a supported configuration.

2. **No native GPU-compute toolchain is installed on the target.** Probed
   2026-06-27: `rocminfo`, `hipcc`, `rocm-smi` absent; no `/opt/rocm*`; `clinfo`
   (OpenCL) absent; no SYCL compiler (`icpx`, `acpp`, `syclcc`, `dpcpp`). A
   validation attempt would first require installing the full ROCm/HIP stack (or a
   SYCL implementation) on unsupported hardware — large effort, low confidence.

3. **`nec_accel` is wgpu-only by design; a native backend is a parallel
   implementation.** Adding ROCm/HIP or SYCL means new FFI bindings, a second
   matrix-fill kernel maintained alongside `zmatrix_fill.wgsl`, a build/feature
   gate, and dependency/license review — for **no correctness gain**: the wgpu
   **Vulkan (RADV)** path is already validated on this exact Renoir GPU
   (PH7-CHK-005 measured real Z-fill/RP kernels here), so cross-vendor AMD
   acceleration is already delivered.

**Revisit trigger (specific):** pursue native ROCm/SYCL only when *either* a
ROCm-supported AMD device (CDNA, or a supported discrete RDNA card) becomes
available for validation, *or* profiling shows the RADV Vulkan matrix-fill path is
the bottleneck at production problem sizes (the PH7-CHK-005 crossover shows the
Vulkan kernel is ~200× faster than CPU up to 1536 segments, so this is not
currently the case). Until then the Vulkan path is the supported AMD backend.

---

## CI Behaviour

The wgpu tests run unconditionally in CI (`--features wgpu` is always set for the
`nec_accel` crate).  On bare-metal CI runners without a GPU or software rasterizer:

- `wgpu_noop_compute_pipeline_succeeds_or_skips_gracefully` returns
  `NoOpPipelineResult::NoAdapterAvailable` and the test passes vacuously.
- All other wgpu tests degrade gracefully (they accept CPU fallback paths).

No backend-specific CI flags are required for any of the four validated backends.

---

## See Also

- [docs/gpu-arch.md](gpu-arch.md) — GPU acceleration architecture decisions
- [docs/roadmap.md](roadmap.md) — PH6-CHK-004 and PH7-CHK-006 checklist entries
- [crates/nec_accel/src/wgpu_device.rs](../crates/nec_accel/src/wgpu_device.rs) — adapter enumeration and pipeline implementation

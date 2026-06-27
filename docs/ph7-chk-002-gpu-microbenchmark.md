---
project: fnec-rust
doc: docs/ph7-chk-002-gpu-microbenchmark.md
status: living
last_updated: 2026-06-27
---

# PH7-CHK-002: in-process GPU microbenchmark

## Requirement / change

Roadmap checklist `PH7-CHK-002` (Phase 7): add an in-process GPU microbenchmark
that isolates WGSL kernel dispatch + execution time from per-process wgpu
device-initialization, recorded as a separate metric from the across-process G5
wall-clock gate (`apps/nec-cli/tests/gpu_benchmark_gate.rs`, which uses best-of-N
timing with a 1.5× limit to absorb device-init noise). Done signal: in-process
dispatch timing is reported separately from device-init; the benchmark artifact
schema documents the new field; the gate is non-flaky across ≥10 CI runs.

## Motivation

The G5 gate spawns a fresh process per measurement, so every `--exec gpu` run
pays the wgpu device-initialization cost (instance + adapter + device, tens of
ms) on top of the actual kernel time. On a few-hundred-ms workload that is a
structural floor that swamps the dispatch cost and made a tight gate flaky
(fixed in PR #245 by loosening to best-of-N / 1.5×). That gate can therefore
only catch *gross* regressions. To watch the kernel-dispatch cost itself, we need
a measurement that pays device-init **once** and then times many dispatches that
reuse the same device, pipeline, and buffers.

## Design

`nec_accel::wgpu_device::microbench_zmatrix_dispatch(segments, freq_hz, reps)`:

1. Acquire the wgpu instance/adapter/device **once**, timing `device_init_us`.
2. Build the Z-matrix-fill pipeline, bind group, and buffers once.
3. Run a few warm-up dispatches (shader compile / lazy init), then `reps` timed
   dispatches — each `encode → submit → poll(Wait)` reusing all resources.
4. Return `GpuMicrobench { device_init_us, dispatch_min_us, dispatch_median_us,
   n_dispatches, n_segments }`, or `None` when no adapter is available.

`dispatch_min_us` / `dispatch_median_us` exclude device-init entirely. Using the
**minimum** over `reps` is the standard denoiser for positive-only wall-clock
noise, which is what makes the metric non-flaky (the G5 cross-process gate cannot
do this because each of its samples is a separate process that re-pays
device-init).

The Z-matrix fill kernel is chosen as the microbenchmark target: it is a single
deterministic compute pass with no host-side normalization, so the measured time
is purely kernel dispatch + execution.

## Artifact schema

`docs/benchmark-artifact-schema.md` gains an optional `gpu_microbench` object on
the top-level artifact:

| Field | Type | Description |
|:------|:-----|:------------|
| `n_segments` | integer | Problem size used for the microbenchmark. |
| `n_dispatches` | integer | Number of timed dispatches. |
| `device_init_us` | integer | One-time wgpu device acquisition, microseconds. |
| `dispatch_min_us` | integer | Best per-dispatch time (device-init excluded). |
| `dispatch_median_us` | integer | Median per-dispatch time. |

This is emitted only on a host with a real wgpu adapter; CI without an adapter
omits it.

The same edit corrects the now-stale `FNEC_ACCEL_STUB_GPU` column in the Exec
modes table (that env var was retired in PH7-CHK-001).

## Tests

- `crates/nec_accel` test `gpu_microbench_isolates_dispatch_from_device_init`:
  runs the microbenchmark; when an adapter is present asserts `device_init_us > 0`
  and `0 < dispatch_min_us ≤ dispatch_median_us`, i.e. the dispatch metric is
  reported separately from device-init and is internally consistent. The
  best-of-N construction makes it non-flaky; skips vacuously with no adapter.

## Test results (2026-06-27, real discrete GPU)

`crates/nec_accel/tests/gpu_microbench.rs`, 160-segment fill, 12 reps:

```
device_init = 60983 µs   dispatch_min = 268 µs   dispatch_median = 303 µs
```

The isolated per-dispatch cost (~0.27 ms) is ~227× smaller than the one-time
device-init (~61 ms) — exactly the component the across-process G5 gate cannot
separate. Run 10× consecutively: `dispatch_min` stayed in 185–268 µs and the test
passed every time (non-flaky). `cargo test --workspace` clean; `cargo clippy
--workspace` clean.

---
project: fnec-rust
doc: docs/benchmarks.md
status: living
last_updated: 2026-05-04
---

# Benchmarks

This is the single canonical benchmark document.
It compares baseline results across three targets and explicitly covers four benchmark modes:

- CPU single-thread
- CPU multithread
- GPU
- Hybrid (CPU multithread + GPU)

## Target Aliases

Use a local, gitignored hosts/env mapping to resolve aliases to real SSH targets.

Example (local only):

```bash
# .benchmark-hosts.env (gitignored)
export FNEC_HOST_LOCAL_ALIAS="local-workstation"
export FNEC_HOST_T480_ALIAS="target-t480"
export FNEC_HOST_PI5_ALIAS="target-pi5"
export FNEC_REMOTE_REPO_SUBDIR="git/fnec-rust"
```

Optional SSH alias resolution (local only):

```sshconfig
Host target-t480
  HostName <private-lan-hostname-or-ip>
  User <local-username>

Host target-pi5
  HostName <private-lan-hostname-or-ip>
  User <local-username>
```

## Source CSVs

Three-target baseline CSVs:

- local-workstation: `tmp/local-baseline-20260427T111026Z.csv`
- target-t480: `tmp/t480-baseline-20260427T101204Z.csv`
- target-pi5: `tmp/pi5-baseline-20260427T101239Z.csv`

All datasets contain 81 rows with 0 non-ok rows.

## Baseline Benchmark Matrix

- Decks: `dipole-freesp-51seg`, `dipole-ground-51seg`, `yagi-5elm-51seg`
- Solvers: `hallen`, `pulse`, `sinusoidal`
- Exec modes: `cpu`, `hybrid`, `gpu`
- Repeats: 3

Total rows: $3 \times 3 \times 3 \times 3 = 81$.

## Mode Provenance

Baseline three-target results in this document come from the exec sweep `cpu`, `hybrid`, `gpu`.

For explicit four-mode coverage, an additional local verification sweep was run on `corpus/frequency-sweep-dipole.nec` with `--solver hallen` and 3 repeats per mode:

- CPU single-thread:
  - `RAYON_NUM_THREADS=1`
  - `--exec cpu`
- CPU multithread:
  - `RAYON_NUM_THREADS=$(nproc)`
  - `FNEC_ACCEL_STUB_GPU=0`
  - `--exec hybrid`
- GPU:
  - `RAYON_NUM_THREADS=$(nproc)`
  - `FNEC_ACCEL_STUB_GPU=1`
  - `--exec gpu`
- Hybrid (CPU multithread + GPU):
  - `RAYON_NUM_THREADS=$(nproc)`
  - `FNEC_ACCEL_STUB_GPU=1`
  - `--exec hybrid`

Note: GPU paths are currently stub/fallback based, so this is execution-path coverage with current backend behavior.

## Method Notes

- Local and remote CSV headers differ slightly (`exec` vs `exec_mode`, timestamp naming), but aggregation fields align.
- Timing includes full CLI invocation path, not just kernel-only execution.
- `hybrid` and `gpu` can include fallback paths; mode counts are included to guard against drift.

## Persistent History

To retain benchmark snapshots over time in a tracked, non-`tmp/` location, use:

```bash
bash ./scripts/pi-benchmark-history.sh append <candidate.csv> benchmarks/pi-benchmark-history.csv
```

Or append automatically after each remote benchmark run:

```bash
FNEC_BENCH_HISTORY="benchmarks/pi-benchmark-history.csv" \
  bash ./scripts/pi-remote-benchmark.sh <user@host>
```

History schema (`benchmarks/pi-benchmark-history.csv`):

- `ingested_at_utc`
- `git_sha`
- `source_csv`
- all original benchmark CSV fields from `scripts/pi-remote-benchmark.sh`

Summarize long-term trend by deck/solver/exec mode:

```bash
bash ./scripts/pi-benchmark-history.sh trend benchmarks/pi-benchmark-history.csv
```

## Baseline Results (Three Targets)

### Solver Average Runtime (ms)

| Target Alias | hallen | pulse | sinusoidal |
|---|---:|---:|---:|
| local-workstation | 487.444 | 128.407 | 142.000 |
| target-t480 | 489.037 | 129.370 | 141.630 |
| target-pi5 | 934.185 | 228.111 | 253.407 |

### Hallen Average By Deck (ms)

| Target Alias | dipole-freesp-51seg | dipole-ground-51seg | yagi-5elm-51seg |
|---|---:|---:|---:|
| local-workstation | 22.778 | 23.111 | 1416.444 |
| target-t480 | 20.333 | 21.000 | 1425.778 |
| target-pi5 | 25.889 | 28.556 | 2748.111 |

### Diagnostic Mode Counts

All targets produced the same routing/fallback pattern:

- `hallen`: 27
- `pulse`: 27
- `sinusoidal->hallen(residual)`: 18
- `sinusoidal->pulse(topology)`: 9

### Relative Performance Ratios

target-pi5 vs local-workstation:

- hallen: $934.185 / 487.444 \approx 1.916\times$
- pulse: $228.111 / 128.407 \approx 1.776\times$
- sinusoidal: $253.407 / 142.000 \approx 1.785\times$

target-pi5 vs target-t480:

- hallen: $934.185 / 489.037 \approx 1.910\times$
- pulse: $228.111 / 129.370 \approx 1.763\times$
- sinusoidal: $253.407 / 141.630 \approx 1.789\times$

## Four-Mode Coverage Results (Local Verification)

Deck: `corpus/frequency-sweep-dipole.nec`  
Solver: `hallen`  
Repeats: 3 runs per mode  
Rows per mode: 15 (5 frequencies $\times$ 3 runs)

| Mode | Runtime config | Avg elapsed_ms |
|---|---|---:|
| CPU single-thread | `RAYON_NUM_THREADS=1`, `--exec cpu` | 13.9 |
| CPU multithread | `RAYON_NUM_THREADS=$(nproc)`, `FNEC_ACCEL_STUB_GPU=0`, `--exec hybrid` | 16.5 |
| GPU | `RAYON_NUM_THREADS=$(nproc)`, `FNEC_ACCEL_STUB_GPU=1`, `--exec gpu` | 14.1 |
| Hybrid (CPU multithread + GPU) | `RAYON_NUM_THREADS=$(nproc)`, `FNEC_ACCEL_STUB_GPU=1`, `--exec hybrid` | 16.8 |

For regression checks, compare candidate runs against baseline with `scripts/pi-benchmark-compare.sh --fail-on-mode-drift`.

## G5 Gate: CPU-vs-GPU Benchmark Gate (PH5-CHK-005)

Gate definition: the GPU RP path must not exceed 1.25× CPU RP elapsed time on the large RP grid deck (GPU ≥ 0.8× CPU speed).

### Deck

`corpus/dipole-freesp-rp-large-grid.nec` — 51-segment half-wave dipole at 14.2 MHz, 37×73 = 2701 observation points.

### CI gate

```
cargo test --test gpu_benchmark_gate
```

Passes when GPU median wall time across 3 repetitions ≤ 1.25× CPU median.

### Offline script gate

```bash
scripts/pi-benchmark-compare.sh --gpu-vs-cpu-max-pct 25 <candidate.csv>
```

Fails if any (deck, solver) combination has GPU avg time > 1.25× CPU avg time.

### Implementation notes

- `run_rp_farfield_batch_wgpu` uses `force_fallback_adapter: false` (production function).
  On machines without a hardware GPU the function returns `None` in ≤ 10 ms and falls back to CPU.
  The software rasterizer (`force_fallback_adapter: true`) is intentionally excluded from the
  production batch path to avoid the ~350 ms llvmpipe shader-compile overhead.
- The batch shader (`rp_farfield_batch.wgsl`) dispatches all N observation points in a single GPU
  submission (ceil(N/64) workgroups, workgroup_size=64), replacing the previous per-point
  encode/submit/poll loop that caused O(N) wgpu round-trip overhead.

### Baseline results (local workstation, 2026-05-04)

Deck: `corpus/dipole-freesp-rp-large-grid.nec` (37×73 = 2701 points)  
Solver: hallen  
Build: debug  
Repeats: 3 (median reported)

| Path | Median elapsed (µs) | Ratio |
|---|---:|---:|
| `--exec cpu` | 364,987 | 1.000× (reference) |
| `--exec gpu` | 417,995 | 1.145× |

Gate limit: ≤ 1.25× CPU. **Result: PASS.**

Note: CPU/GPU on this machine differ by ~53 ms (wgpu hardware adapter enumeration overhead)
even when the GPU dispatch falls back to CPU for the MoM solve. On Pi5 (no discrete GPU),
the wgpu init fast-fails and both paths run at identical CPU-only speed.
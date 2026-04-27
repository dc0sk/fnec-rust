---
project: fnec-rust
doc: docs/benchmarks.md
status: living
last_updated: 2026-04-27
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
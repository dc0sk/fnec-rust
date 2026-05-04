---
project: fnec-rust
doc: docs/benchmark-artifact-schema.md
status: living
last_updated: 2026-05-04
---

# Benchmark Artifact Schema

This document defines the JSON schema for benchmark artifacts produced by
`scripts/run-benchmark-matrix.sh` and consumed by `scripts/benchmark-compare-json.sh`.
The schema version is `"1"`.

## Top-level object

| Field | Type | Description |
|:------|:-----|:------------|
| `schema_version` | string | Always `"1"` for this schema revision. |
| `generated_at` | string | ISO 8601 UTC timestamp of when the artifact was produced. |
| `git_sha` | string | Short git SHA of the HEAD commit at generation time. |
| `runner_nproc` | integer | Logical CPU count on the runner (`nproc`). |
| `runs` | array of Run | Individual timed invocations (one per repeat). |
| `summary` | array of Summary | Aggregated statistics per (deck, solver, exec\_mode). |

## Run object

Each element of the `runs` array represents one timed CLI invocation.

| Field | Type | Description |
|:------|:-----|:------------|
| `deck` | string | Basename of the NEC deck file without `.nec` extension. |
| `solver` | string | Solver name passed to `--solver` (e.g. `hallen`, `pulse`). |
| `exec_mode` | string | One of `cpu-single`, `cpu-multi`, `gpu`. See Exec modes below. |
| `elapsed_ms` | integer | Wall-clock elapsed time in milliseconds for that single run. |

## Summary object

Each element of the `summary` array aggregates all repeat runs for one
(deck, solver, exec\_mode) combination.

| Field | Type | Description |
|:------|:-----|:------------|
| `deck` | string | Same as Run.deck. |
| `solver` | string | Same as Run.solver. |
| `exec_mode` | string | Same as Run.exec\_mode. |
| `n_runs` | integer | Number of repeated runs included in this summary. |
| `avg_ms` | integer | Mean elapsed time in milliseconds (integer truncation). |
| `min_ms` | integer | Minimum elapsed time across repeats. |
| `max_ms` | integer | Maximum elapsed time across repeats. |

## Exec modes

| `exec_mode` value | CLI `--exec` arg | `RAYON_NUM_THREADS` | `FNEC_ACCEL_STUB_GPU` | Description |
|:------------------|:-----------------|:--------------------|:----------------------|:------------|
| `cpu-single`      | `cpu`            | `1`                 | `0`                   | Deterministic single-thread CPU baseline. |
| `cpu-multi`       | `hybrid`         | `$(nproc)`          | `0`                   | Multi-threaded CPU sweep via parallel FR lanes. |
| `gpu`             | `gpu`            | `$(nproc)`          | `1`                   | GPU dispatch path (stub fallback in CI; real wgpu on hardware). |

## Example

```json
{
  "schema_version": "1",
  "generated_at": "2026-05-04T12:00:00Z",
  "git_sha": "917f625",
  "runner_nproc": 4,
  "runs": [
    { "deck": "dipole-freesp-51seg", "solver": "hallen", "exec_mode": "cpu-single", "elapsed_ms": 23 },
    { "deck": "dipole-freesp-51seg", "solver": "hallen", "exec_mode": "cpu-single", "elapsed_ms": 22 },
    { "deck": "dipole-freesp-51seg", "solver": "hallen", "exec_mode": "cpu-single", "elapsed_ms": 24 }
  ],
  "summary": [
    {
      "deck": "dipole-freesp-51seg",
      "solver": "hallen",
      "exec_mode": "cpu-single",
      "n_runs": 3,
      "avg_ms": 23,
      "min_ms": 22,
      "max_ms": 24
    }
  ]
}
```

## Regression comparison

`scripts/benchmark-compare-json.sh` compares a candidate artifact against a stored
baseline (by default `benchmarks/ci-baseline.json`) using thresholds from
`.benchmark-gates.toml`:

- **Timing regression**: for each matching `(deck, solver, exec_mode)` triple, the
  candidate `avg_ms` must not exceed the baseline `avg_ms` by more than
  `max_regression_pct` percent.
- **GPU/CPU ratio**: for each `(deck, solver)` pair, the candidate `gpu avg_ms`
  divided by the candidate `cpu-single avg_ms` must not exceed `max_gpu_cpu_ratio`.

## Updating the baseline

To update `benchmarks/ci-baseline.json` with fresh numbers from the local machine:

```bash
cargo build --release -p nec-cli
bash scripts/run-benchmark-matrix.sh benchmarks/ci-baseline.json
git add benchmarks/ci-baseline.json
git commit -m "chore: update CI benchmark baseline"
```

The new baseline takes effect for all subsequent CI runs once merged to `main`.

---
title: Benchmark Baseline Comparison (Local, T480, Pi 5)
status: draft
last-updated: 2026-04-27
---

# Benchmark Baseline Comparison

This document compares the same benchmark sweep across three hosts:

- Local workstation (this machine)
- Lenovo T480 (`dc0sk@192.168.121.50`)
- Raspberry Pi 5 (`dc0sk@192.168.121.49`)

## Source CSVs

- Local: `tmp/local-baseline-20260427T111026Z.csv`
- T480: `tmp/t480-baseline-20260427T101204Z.csv`
- Pi 5: `tmp/pi5-baseline-20260427T101239Z.csv`

All three datasets contain 81 rows with 0 non-ok rows.

## Run Shape

- Decks: `dipole-freesp-51seg`, `dipole-ground-51seg`, `yagi-5elm-51seg`
- Solvers: `hallen`, `pulse`, `sinusoidal`
- Exec modes: `cpu`, `hybrid`, `gpu`
- Repeats: 3

Total rows: $3 \times 3 \times 3 \times 3 = 81$.

## Solver Average Runtime (ms)

| Host | hallen | pulse | sinusoidal |
|---|---:|---:|---:|
| Local workstation | 487.444 | 128.407 | 142.000 |
| T480 | 489.037 | 129.370 | 141.630 |
| Pi 5 | 934.185 | 228.111 | 253.407 |

## Hallen Average By Deck (ms)

| Host | dipole-freesp-51seg | dipole-ground-51seg | yagi-5elm-51seg |
|---|---:|---:|---:|
| Local workstation | 22.778 | 23.111 | 1416.444 |
| T480 | 20.333 | 21.000 | 1425.778 |
| Pi 5 | 25.889 | 28.556 | 2748.111 |

## Diagnostic Mode Counts

Each host produced the same routing/fallback pattern:

- `hallen`: 27
- `pulse`: 27
- `sinusoidal->hallen(residual)`: 18
- `sinusoidal->pulse(topology)`: 9

This indicates no mode drift between hosts for this benchmark matrix.

## Relative Performance Summary

Pi 5 vs local workstation:

- hallen: $934.185 / 487.444 \approx 1.916\times$
- pulse: $228.111 / 128.407 \approx 1.776\times$
- sinusoidal: $253.407 / 142.000 \approx 1.785\times$

Pi 5 vs T480:

- hallen: $934.185 / 489.037 \approx 1.910\times$
- pulse: $228.111 / 129.370 \approx 1.763\times$
- sinusoidal: $253.407 / 141.630 \approx 1.789\times$

Local workstation and T480 are effectively tied for this corpus/mode set.

## Notes And Caveats

- Local CSV schema differs slightly from remote CSVs (`timestamp_unix_ms` and `exec` column name), but semantic fields needed for aggregation are aligned.
- Timing includes full CLI invocation and report generation path, not just solver kernel time.
- `gpu` and `hybrid` currently include fallback behavior depending on execution path availability; use diagnostic mode counts above to ensure comparisons are mode-consistent.
- For regression gates, compare candidate vs baseline with `scripts/pi-benchmark-compare.sh` and enforce `--fail-on-mode-drift`.

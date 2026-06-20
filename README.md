# fnec-rust Benchmark Dashboard

This branch hosts the CI benchmark dashboard published at
`https://dc0sk.github.io/fnec-rust/`.

## How it works

1. The `benchmark-dashboard.yml` workflow runs on every push to `main`.
2. It runs the three-mode benchmark matrix via `scripts/run-benchmark-matrix.sh`.
3. Results are compared against the baseline in `benchmarks/ci-baseline.json`.
4. The JSON artifact is uploaded and the step summary is written.
5. A future enhancement will commit results to this branch for the dashboard.

## Updating the dashboard page

Edit `index.html` in this branch and push. The page updates immediately.

## Benchmark artifact schema

See `docs/benchmark-artifact-schema.md` in the `main` branch.

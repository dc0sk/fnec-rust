---
project: fnec-rust
doc: docs/project/tooling-catalog.md
status: living
last_updated: 2026-07-02
---

# Helper & validation tooling catalog

The **tooling layer**: the scripts, harnesses, corpus, and external reference
engines that produce and defend the test evidence. These sit alongside the test
layer — they generate references, gate contracts, and benchmark performance.

## Release / contract gates (`scripts/`)

| Tool | Purpose | Gates |
|:-----|:--------|:------|
| `validate-docs-frontmatter.sh` | Validate the 4-key frontmatter block on every `docs/*.md` | Doc contract |
| `validate-doc-frontmatter.sh` | Thin alias delegating to the plural script | Doc contract |
| `check-version-bump-docs.sh` | A `Cargo.toml` version bump must ship changelog + releasenotes + SBOM changes | Release contract |
| `stamp-doc-last-updated.sh` | Stamp `last_updated` on changed docs for PR automation | Doc contract |

## Benchmark harnesses (`scripts/`)

| Tool | Purpose |
|:-----|:--------|
| `run-benchmark-matrix.sh` | Run the three-mode matrix (CPU single-thread / CPU multi-thread / GPU) |
| `benchmark-compare-json.sh` | Compare two benchmark JSON artifacts and compute regression deltas |
| `test-benchmark-gate.sh` | Exercise the benchmark regression gate locally |
| `pi-benchmark-compare.sh` | Raspberry Pi CPU/GPU comparison (NFR-001a reference hardware) |
| `pi-benchmark-history.sh` | Track Pi benchmark history across runs |
| `pi-benchmark-summary.sh` | Summarize Pi benchmark results |
| `pi-remote-benchmark.sh` | Drive a benchmark on a remote Pi over SSH |
| `pi-remote-workspace-check.sh` | Verify the remote Pi workspace before a remote run |

## Reference-data helpers (`scripts/`)

| Tool | Purpose |
|:-----|:--------|
| `import-reference-impedance.py` | Import external (NEC2/4nec2) reference impedances into `corpus/reference-results.json` |
| `fix_patterns.py` | Batch-fix/normalize corpus RP pattern-sample data |

## In-repo example harnesses

| Harness | Purpose | Backs |
|:--------|:--------|:------|
| `apps/nec-cli/examples/gpu_crossover.rs` | Measure the real discrete-GPU vs CPU crossover; emits `benchmarks/real-gpu-crossover.json` | PH7-CHK-005 |

## Corpus & reference data (`corpus/`)

The corpus is the numerical ground truth (NFR-004, COMP-002/008). ~40 `.nec`
decks plus:

| Artifact | Role |
|:---------|:-----|
| `corpus/reference-results.json` | Golden reference values + per-metric tolerances; consumed by `corpus_validation.rs` |
| `corpus/reference-import-template.json` | Template for importing new external references |
| `corpus/README.md` | Corpus conventions and provenance |
| `corpus/dipole-*.nec`, `yagi-*.nec`, `tl-*.nec`, `multi-source-*.nec` | Deck fixtures spanning free-space/ground/loaded/RP/TL/multi-source/GM-GR classes |

Benchmark artifacts live under `benchmarks/` (`ci-baseline.json`,
`real-gpu-crossover.json`).

## External validation engines

Cross-tool validation references (not vendored; invoked from the host):

| Tool | Role | Notes |
|:-----|:-----|:------|
| `nec2c` (`/usr/bin/nec2c`) | NEC-2 reference engine for external parity | Has an input-path-length limit — use short paths like `/tmp/nec/`. For source-free scattering decks (plane wave) an `XQ` execute card is required to print the induced `CURRENTS AND LOCATION` table. |
| Python MoM reference (`hallen_reference.py`) | Independent Hallén cross-check | Phase 0 baseline validation |
| xnec2c / 4nec2 | Mainstream comparators | Design/behaviour references (roadmap parity tables) |

### PH8-CHK-002 external reference (in-flight)

`docs/dev/ph8-planewave-ref-theta30.nec` — a checked-in `nec2c` harness deck
(`EX 1 1 1 0 30 0 0` + `XQ 0`) that makes `nec2c` solve and print the induced
currents for a θ=30° incident plane wave. This is the external parity reference for
the PH8-CHK-002 plane-wave RHS work.

## Software-bill-of-materials

`SBOM.spdx.json` (regenerated via `cargo sbom > SBOM.spdx.json`) — required to
change on any version bump by `check-version-bump-docs.sh`; backs DEC-007 license
tracking.

---
project: fnec-rust
doc: docs/project/test-results.md
status: living
last_updated: 2026-07-02
---

# Test results

The **results layer**: recorded outcomes of the test/gate runs that back the
traceability claims. Each entry pins the commit, toolchain, command, and result so
a claim of "passing" is auditable, not asserted.

Update this file before each push whenever code or tests changed (see the pre-push
rule in [README.md](README.md)).

## Latest recorded run

| Field | Value |
|:------|:------|
| Date | 2026-07-02 |
| Commit | branch `feat/ph8-chk-002-planewave-foundation` (base `84fcc9d` main) |
| Version | fnec-rust 0.7.0 |
| Toolchain | rustc 1.94.1 (e408947bf 2026-03-25) |
| Host | Linux 6.18 x86_64 (AMD Renoir gfx90c APU, RADV Vulkan) |

### `cargo test --workspace` (default features)

```
540 passed; 0 failed; 0 ignored — across 53 test binaries
exit code 0
```

540 = the 539 baseline + 1 new parser test (`ex_plane_wave_polarization_f3_is_captured`)
from the PH8-CHK-002 code foundation. No corpus/reference contract changed
(EX types 1–5 still fail fast with the preserved `is not yet supported` substring).

Authoritative workspace pass count. Covers all crates and both apps with the CPU
solver path and the GPU dispatch seam in CPU-fallback mode (wgpu feature off).

### `cargo test -p nec_accel --features wgpu` (real GPU dispatch)

```
29 passed; 0 failed; 0 ignored — across 6 test binaries
exit code 0
```

Exercises the real wgpu device path (adapter enumeration, WGSL Z-fill, RP
far-field, and the GPU-resident Hallén solve) on the AMD RADV RENOIR Vulkan
backend — not the CI software rasterizer. This is the evidence behind the Phase 7
GPU-residency claims (PH7-CHK-003/004) and gates G6/G7.

### `cargo clippy --workspace`

```
exit code 0 — clean (no warnings)
```

## Standing CI gates (contract-level, always-on)

| Gate | Script / test | Enforces |
|:-----|:--------------|:---------|
| Corpus tolerance | `apps/nec-cli/tests/corpus_validation.rs` + `corpus/reference-results.json` | NFR-004, COMP-002/008 — impedance/gain/pattern/current within tolerance |
| Report contract | `report_contract.rs`, `scriptability_contract.rs` | FR-005, NFR-005 — stable machine-parseable output |
| Doc frontmatter | `scripts/validate-docs-frontmatter.sh` | Every `docs/*.md` has the 4-key frontmatter block |
| Version-bump docs | `scripts/check-version-bump-docs.sh` | A `Cargo.toml` version bump requires changelog + releasenotes + SBOM changes |
| Dependency licenses | `deny.toml` (`cargo deny check licenses`) | GAP-008/BLK-005 SPDX allowlist |
| Benchmark regression | `.benchmark-gates.toml` + `.github/workflows/benchmark-dashboard.yml` | PH6-CHK-001 regression-delta thresholds |

## Milestone gate evidence (historical, from roadmap)

Point-in-time numerical evidence recorded at delivery; re-verified by the standing
tests above on every run.

| Gate | Evidence |
|:-----|:---------|
| G6 (GPU Z-fill parity) | max rel err 2.12×10⁻⁶ vs CPU (limit 1×10⁻⁴), 51-seg dipole @ 14 MHz |
| G7 (GPU fill + CPU solve) | ΔR=0 Ω, ΔX=0 Ω vs all-CPU reference |
| PH7-CHK-003 (GPU-resident solve) | ΔR=0.012 Ω, ΔX=0.002 Ω vs f64 CPU; 3 corpus decks ≤0.01 Ω |
| PH7-CHK-002 (microbench) | 61 ms device-init vs 268 µs dispatch (~227× isolation); 10/10 non-flaky |
| PH7-CHK-005 (real GPU crossover) | Z-fill: GPU beats CPU <32 seg, up to ~240× at 1536 seg; RP 1.5–1.8× faster |
| Reference dipole (Phase 0 baseline) | 51-seg λ/2 dipole → 74.24 + j13.90 Ω (matches Python reference) |

## How to reproduce

```sh
# Full workspace (CPU path):
cargo test --workspace

# Real GPU path (needs a wgpu-capable adapter):
cargo test -p nec_accel --features wgpu

# Lint:
cargo clippy --workspace

# Doc frontmatter + version-bump gates:
scripts/validate-docs-frontmatter.sh
scripts/check-version-bump-docs.sh
```

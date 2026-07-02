---
project: fnec-rust
doc: docs/project/architecture-design-index.md
status: living
last_updated: 2026-07-02
---

# Architecture & design index

The **design layer** of the traceability chain: the documents that record *how*
and *why* the implementation is shaped the way it is. Every non-trivial checklist
item that made an architectural choice has a design/decision record; this index
maps decision → document.

## Foundational architecture

| Document | Scope |
|:---------|:------|
| `docs/architecture.md` | System architecture: crate boundaries, data flow, frontend/solver separation. |
| `docs/design.md` | Design principles and cross-cutting decisions. |
| `docs/applied-math.md` | The MoM/Hallén numerical formulation, quadrature, singularity handling. |
| `docs/nec-requirements.md` | NEC deck/reference-data requirements for tolerance verification. |
| `docs/project-blueprint.md` | High-level product blueprint. |
| `docs/steering.md` | Steering/process guidance. |

## Domain decision records

| Document | Decision recorded | Requirement IDs |
|:---------|:------------------|:----------------|
| `docs/nec4-support.md` | NEC-4 feature boundary: supported/deferred card set; card status index | GAP-002, BLK-002, COMP-003 |
| `docs/card-support-matrix.md` | Per-card support state (Full/Partial/Deferred) | COMP-003, CP-003 |
| `docs/corpus-validation-strategy.md` | Corpus + tolerance strategy; NEC-5-informed matrix (`PH2N5-*`, `PH6N5-*`) | PRT-010, GAP-013 |
| `docs/gpu-arch.md` | GPU architecture: wgpu choice, target matrix, G1–G7 gate sequence | GAP-007, DEC-003 |
| `docs/multi-vendor-gpu.md` | Backend matrix; AMD Vulkan validation; ROCm/SYCL deferral | DEC-008, CP-009 |
| `docs/distributed-execution-design.md` | Transport, authN/authZ, worker contract, result-cache design | PRT-011, CP-011 |
| `docs/worker-deployment.md` | SSH worker deployment, hosts config, capability cache | PRT-011 |
| `docs/nec5-frontier.md` | NEC-5 accuracy frontier decision (wire-only continuation) | PRT-009, CP-009 |
| `docs/plugin-api-design.md` | Extension surface + safety model; EP-1..4 | GAP-004, BLK-004, DEC-006 |
| `docs/dependency-policy.md` | SPDX allowlist/deny-list, GPLv2 rules, exception process | GAP-008, BLK-005, DEC-007 |
| `docs/json-output-schema.md` | Locked JSON output schema v1 | FR-008, PH4-CHK-003 |
| `docs/project-format.md` | TOML/Markdown project file format | FR-004, GAP-010/015 |
| `docs/phase5-entry-criteria.md` | Measurable Phase 5 (GPU) entry gate | PH4-CHK-007 |
| `docs/rooftop-basis-plan.md` | Basis-function strategy notes | Solver accuracy |
| `docs/solver-findings.md` | Solver numerical findings log | NFR-004 |
| `docs/utd-feasibility-assessment.md` | UTD feasibility assessment | PRT-009 (frontier) |

## Per-checklist decision records (Phase 7–8)

These are the fine-grained "requirement → decision → implementation → result"
records for the most recent work. Each is self-contained and is the design node
in its matrix row.

| Document | Checklist | Decision |
|:---------|:----------|:---------|
| `docs/ph7-chk-001-gpu-stub-retirement.md` | PH7-CHK-001 | Retire the GPU CPU-emulation scaffold; no path reports CPU time as GPU time. |
| `docs/ph7-chk-002-gpu-microbenchmark.md` | PH7-CHK-002 | In-process microbench isolating dispatch from device-init. |
| `docs/ph7-chk-003-gpu-resident-solve.md` | PH7-CHK-003 | GPU-resident f32 solve; Björck refinement; 2 Ω tolerance bar (amended). |
| `docs/ph7-chk-004-distributed-gpu-execution.md` | PH7-CHK-004 | `--exec gpu` through the SSH worker pool; serde-default protocol fields. |
| `docs/ph7-chk-005-real-gpu-benchmark.md` | PH7-CHK-005 | Real AMD-GPU crossover evidence; not the CI software rasterizer. |
| `docs/ph8-chk-002-plane-wave-excitation.md` | PH8-CHK-002 | **NEC2 EX-type alignment** + plane-wave RHS in the forcing term; staged delivery. |

## Notable architectural decisions (verified)

- **wgpu as the single GPU API** (DEC-003, GAP-007): one WGSL codebase covers
  Vulkan/Metal/DX12/OpenCL; native ROCm/SYCL deferred with a dated rationale
  because the AMD Renoir APU target is outside ROCm's support matrix
  (`docs/multi-vendor-gpu.md`, PH7-CHK-006).
- **f64 CPU solve is the accuracy reference; GPU is f32** (PH7-CHK-003): the
  GPU-resident solve validates to 2 Ω, not the 0.05 Ω corpus gate, because wgpu is
  f32-only and the Hallén solve is normal-equations least-squares.
- **Wire-only NEC-5 continuation** (PRT-009, `docs/nec5-frontier.md`): surfaces
  are explicitly out of scope; Phase 8 finishes wire-card semantics instead.
- **NEC2 EX-type alignment** (PH8-CHK-002, user-approved 2026-06-27): fnec's
  EX-type numbering is realigned to NEC2 (type 1 = plane wave, current source →
  type 4) so real 4nec2 plane-wave decks are not misread. This is the current
  in-flight decision.

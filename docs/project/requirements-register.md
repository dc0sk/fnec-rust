---
project: fnec-rust
doc: docs/project/requirements-register.md
status: living
last_updated: 2026-07-02
---

# Requirements register

Consolidated index of every requirement, scope decision, parity gap, competitive
item, gap, and blocker ID used across the project, with its source document and
current coverage state. This is the **left edge** of the traceability chain — the
["what must be true"] that the rest of the chain satisfies.

Detail lives in the source docs; this register exists so no ID is orphaned and so
the [traceability-matrix.md](traceability-matrix.md) can point at a single
canonical list.

## Scope decisions (`DEC-*`) — source: `docs/requirements.md`

| ID | Decision | Coverage |
|:---|:---------|:---------|
| DEC-001 | Support NEC-2 and NEC-4 incrementally | Phases 1–2, ongoing Phase 8 |
| DEC-002 | Ground scope starts simple, complexity added later | Phase 2 (GN0/GN1/GN2), Phase 8 (PH8-CHK-006) |
| DEC-003 | GPU via wgpu for the full Hallén path, optional with CPU fallback | Phases 5–7 |
| DEC-004 | Text output first; JSON/CSV later | Phase 1 text, Phase 4 JSON (PH4-CHK-003) |
| DEC-005 | Modern task-oriented GUI, not a 4nec2 clone | Phase 3 (PH3-CHK-009..012) |
| DEC-006 | Plugin/scripting in scope | Phase 4 (EP-1..4) |
| DEC-007 | License risk tracked via SBOM + dependency review | Phase 4 (PH4-CHK-001), `deny.toml`, SBOM gate |
| DEC-008 | GPU prioritizes FOSS frameworks; AMD preferred | Phases 5–7 (wgpu Vulkan on AMD; ROCm/SYCL deferral) |
| DEC-009 | Explicit parity targets vs NEC-2/4, 4nec2, EZNEC, AutoEZ, xnec2c, necpp | Roadmap parity tables; ongoing |
| DEC-010 | Hallén supports non-collinear/junctioned multi-wire (hybrid formulation) | Phase 1 close (v0.3.0) |
| DEC-011 | Sinusoidal mode must be safety-bounded with Hallén fallback | Phase 6 (PH6-CHK-003) |

## Functional requirements (`FR-*`) — source: `docs/requirements.md`

| ID | Requirement | Coverage |
|:---|:------------|:---------|
| FR-001 | Core solver as reusable Rust crates | `nec_parser/model/solver/report/project/accel/worker` |
| FR-002 | CLI and GUI frontends | `apps/nec-cli`, `apps/nec-gui` |
| FR-003 | Parse and execute real 4nec2 NEC decks | Phases 1–2; Phase 8 closes remaining cards |
| FR-004 | Markdown project import/export | GAP-015 (done: `nec_project` `from/to_markdown`) |
| FR-005 | 4nec2-like text reports | Phase 1 report contract v1 (PH2-CHK-004) |
| FR-006 | Plugin/scripting extension mechanism | Phase 3–4 (EP-1..4) |
| FR-007 | Deterministic batch/sweep workflows | Phase 3 (PH3-CHK-006/007/008) |
| FR-008 | Stable automation-oriented core APIs | Phase 4 (PH4-CHK-003/004/006) |
| FR-009 | Early geometry diagnostics | Phase 2 (PH2-CHK-006) |
| FR-010 | Resonance/convergence/matching helpers | Phase 3 (PH3-CHK-008) |

## Non-functional requirements (`NFR-*`) — source: `docs/requirements.md`

| ID | Requirement | Coverage |
|:---|:------------|:---------|
| NFR-001 | Linux-first (Wayland), then macOS, then Windows | CI/dev on Linux; portability via `wgpu`/`iced` |
| NFR-001a | Raspberry Pi 4/5 as in-scope acceleration reference | `scripts/pi-*`, benchmark matrix |
| NFR-002 | CPU execution multithreaded and deterministic | rayon paths; `RAYON_NUM_THREADS` benchmark mode |
| NFR-003 | GPU optional with reliable CPU fallback | Phase 5–7; `dispatch_frequency_point` fallback |
| NFR-004 | Numerical compatibility measured with per-metric tolerance | Corpus + `reference-results.json` gates |
| NFR-005 | Script-friendly stable stdin/stdout/stderr | Phase 2 scriptability contract (PH2-CHK-008) |
| NFR-006 | Usability competitive with incumbents | Phase 3 usability benchmark (PH3-CHK-012) |

## Compatibility requirements (`COMP-*`) — source: `docs/requirements.md`

| ID | Requirement | Coverage |
|:---|:------------|:---------|
| COMP-001 | Tolerant parsing of real 4nec2 decks | `nec_parser`; corpus deck sanity |
| COMP-002 | Output comparable to reference within tolerance matrix | Corpus validation gate |
| COMP-003 | NEC-4 scope versioned and explicit | `docs/nec4-support.md`, `docs/card-support-matrix.md` |
| COMP-004..007 | xnec2c secondary dialect, auto-detection, override, isolation | Parser dialect design (staged) |
| COMP-008 | Accuracy never regresses below tolerance without contract change | CI corpus gate |
| COMP-009 | Track 4nec2/EZNEC mainstream parity | Roadmap parity tables |
| COMP-010 | Track xnec2c/yeti01-nec2/necpp | Roadmap parity tables |
| COMP-011 | CLI sufficient to replace classic batch NEC tools | Phase 2 (PH2-CHK-008), GAP-011 |
| COMP-012 | Embeddable automation surface (necpp-style) | Phase 4 bindings (PH4-CHK-004) |

## Parity gaps (`PRT-*`) — source: `docs/roadmap.md` / `docs/requirements.md`

| ID | Gap | Target phase | State |
|:---|:----|:-------------|:------|
| PRT-001 | Ground modeling short of NEC-4/EZNEC | Phase 2 | Partial; Phase 8 (PH8-CHK-006) extends |
| PRT-002 | Loads/TL/network/source families lag | Phase 2–3 | Partial; Phase 8 (PH8-CHK-001..005) closes |
| PRT-003 | Sweep/gain/pattern/report weaker than 4nec2 | Phase 1–2 | Done |
| PRT-004 | GUI/workflow maturity behind | Phase 3 | Done |
| PRT-005 | No optimization/parameter-sweep workflow | Phase 3–4 | Done |
| PRT-006 | No AutoEZ-class automation | Phase 3–4 | Done |
| PRT-007 | Weak geometry validation/embeddability | Phase 2–4 | Done |
| PRT-008 | Accuracy program too narrow | Phase 1–3 | Done (broadened corpus) |
| PRT-009 | No NEC-5 surfaces plan | Phase 4–5 | Decided: wire-only (`nec5-frontier.md`) |
| PRT-010 | No NEC-5-informed validation matrix | Phase 2–3 | Done (PH2-CHK-007) |
| PRT-011 | No distributed authenticated execution | Phase 5 | Done (Phase 6–7) |

## Competitive parity work items (`CP-*`) — source: `docs/roadmap.md`

`CP-001`..`CP-012`: full descriptions and comparators in the roadmap
"Competitive parity work items" table. Cross-referenced by checklist rows in the
[traceability-matrix.md](traceability-matrix.md). Notable active item: **CP-003**
(missing source/TL/network cards break deck portability) is the driving item for
Phase 8.

## Gaps (`GAP-*`) and blockers (`BLK-*`) — source: `docs/roadmap.md`, `docs/requirements.md`

| ID | Title | State |
|:---|:------|:------|
| GAP-002 | NEC-4 feature boundary | Resolved (`docs/nec4-support.md`) |
| GAP-003 | MVP ground model set | Resolved (Phase 2) |
| GAP-004 | Plugin/scripting interface | Resolved (BLK-004) |
| GAP-005 | 4nec2-like text report | Resolved 2026-04-23 |
| GAP-006 | GUI information architecture | Resolved (Phase 3) |
| GAP-007 | GPU rollout criteria | Resolved 2026-05-03 (`docs/gpu-arch.md`) |
| GAP-008 | Dependency/license policy | Resolved (BLK-005) |
| GAP-009 | Workflow parity acceptance criteria | Resolved (PH3-CHK-012) |
| GAP-010 | Automation/embedding strategy | Resolved (Phase 4) |
| GAP-011 | Classic batch-CLI parity definition | Resolved (Phase 2) |
| GAP-012 | AutoEZ-class automation acceptance | Resolved (Phase 3) |
| GAP-013 | NEC-5-informed validation matrix | Resolved (PH2-CHK-007) |
| GAP-014 | External optimizer-loop compatibility | Resolved (Phase 3) |
| GAP-015 | Markdown project import/export | Resolved (`nec_project`) |
| BLK-001 | Tolerance matrix defined | Resolved 2026-04-22 |
| BLK-002 | NEC-4 feature boundary documented | Resolved |
| BLK-003 | 4nec2 report contract locked | Resolved 2026-04-23 |
| BLK-004 | Plugin API + first two extension points | Resolved 2026-04-30 |
| BLK-005 | GPLv2 dependency policy | Resolved 2026-05-02 |

## Checklist ID families (delivery units)

The requirement IDs above are satisfied through **checklist items**, the natural
traceability unit (each already carries requirement IDs + validation artifacts in
the roadmap):

- `PH1-CHK-*` … `PH8-CHK-*` — per-phase implementation checklists (`docs/roadmap.md`).
- `G1`..`G7` — GPU milestone gates (Phase 5, `docs/gpu-arch.md`).
- `PH2N5-*`, `PH6N5-*` — NEC-5-informed validation matrix rows
  (`docs/corpus-validation-strategy.md`).
- `EP-1`..`EP-4` — plugin extension points (`docs/plugin-api-design.md`).

Phase 8 (`PH8-CHK-001..006`) is the only phase with open items;
**PH8-CHK-002** (incident plane-wave excitation) is in progress — see its
design record `docs/ph8-chk-002-plane-wave-excitation.md`.

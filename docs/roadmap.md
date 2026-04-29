---
project: fnec-rust
doc: docs/roadmap.md
status: living
last_updated: 2026-04-29
---

# Roadmap

## Roadmap principles

- **Staged delivery**: Each phase builds on prior phases; no phase skips its blockers.
- **Tolerance-first testing**: Numerical results are measured against a curated reference corpus with explicit tolerance per metric (impedance, gain, pattern, current). No solver expansion without validated corpus pass.
- **Explicit scope boundaries**: NEC-4 support is versioned and partial; users must see what is and isn't supported. Ground models, source types, and frequency sweeps each have documented roadmaps.
- **FOSS and diversity prioritized**: GPU acceleration favours FOSS frameworks (OpenCL, SYCL, HIP) and AMD GPU support to avoid vendor lock-in.

## Competitive parity target

fnec-rust is not aiming for "good enough for a Rust rewrite". The target is to be at least equal to the established NEC ecosystem in three dimensions:

- **Accuracy**: equal to or better than NEC-2/NEC-4 for supported problems, with documented tolerance gates and reference-corpus proof.
- **Features**: equal to the practical modeling scope users expect from 4nec2 and EZNEC for mainstream antenna work, while incrementally closing the remaining NEC-4/NEC-5 gaps.
- **Usability**: equal to or better than 4nec2 and EZNEC in workflow speed, diagnostics, visualization, and repeatability, not merely raw solver output.

## External baseline comparison

| Baseline | Strengths users already have there | Where fnec-rust is currently behind | Impact if parity is the goal | Roadmap consequence |
|:---------|:-----------------------------------|:------------------------------------|:-----------------------------|:--------------------|
| NEC-2 | Public reference, broad deck compatibility, established impedance/pattern expectations, transmission lines and loads in the classic workflow | Parser/solver breadth still incomplete; corpus does not yet validate loads, sweep reporting, patterns, or transmission-line/network features | Without NEC-2 parity, fnec-rust cannot credibly claim compatibility for real-world deck exchange | Finish Phase 1/2 around complete NEC-2 mainstream coverage before widening scope |
| NEC-4 | Better handling of near-ground and buried wires, improved Sommerfeld ground, current sources, larger problem handling, broader production feature set | Ground support is still limited; buried/near-ground accuracy, current-source families, and more of the NEC-4 card space are not yet production-ready | This blocks serious low-band, vertical, buried radial, and lossy-ground work; users will keep external engines in the loop | Promote advanced ground and NEC-4 source support to first-class Phase 2 deliverables |
| NEC-5 | Mixed-potential improvements, wires plus surfaces, buried conductors, fewer stepped-diameter and placement sensitivities, better behavior on difficult models | No NEC-5-class surface modeling, no stepped-diameter mitigation strategy, no explicit roadmap for difficult-geometry accuracy beyond Hallen thin-wire work | "Better than NEC-2" is not enough if users expect modern robustness on small, buried, or mixed wire/surface structures | Add an explicit long-range accuracy program: difficult geometry corpus, surfaces strategy, and sensitivity regression tracking |
| xnec2c | Active open-source NEC2 implementation with mature GUI workflow, near/far-field views, built-in editor, sweep-oriented interaction, Smith/VSWR style displays, and SMP frequency-loop parallelism | fnec-rust still lacks equivalent visualization, editing workflow, and interactive sweep inspection; xnec2c remains ahead as an everyday Linux/Unix open-source workbench | Even with strong core numerics, fnec-rust will feel incomplete to open-source users who expect an integrated workbench, not just a solver binary | Treat xnec2c as the open-source workflow baseline for Linux and bring plotting, sweep inspection, and model-edit cycle into the product roadmap |
| xnec2c-optimize | Open-source optimizer loop for xnec2c with simplex-driven variable tuning, VSWR/gain-oriented objective workflows, and repeatable optimizer-run automation from config files | fnec-rust lacks equivalent optimizer-loop ergonomics and objective-driven run orchestration for external or native optimization drivers | Users who depend on optimizer-driven antenna iteration will still need external scripts/tools even if base sweep support exists | Track optimizer-loop compatibility as a first-class automation target and expose deterministic CLI/API hooks for external optimizers |
| yeti01/nec2 | Minimal modernized UNIX NEC2 based on original FORTRAN with double precision, Sommerfeld/Norton ground support, and straightforward file-based CLI operation | fnec-rust is not yet ahead on breadth of classic NEC2 execution confidence because some mainstream NEC2 cards and deck classes are still incomplete | If fnec-rust cannot at least replace a clean open NEC2 CLI for routine batch work, its compatibility story remains weak | Keep classic batch-CLI compatibility and mainstream NEC2 card coverage as a hard Phase 1-2 bar |
| necpp | Embeddable NEC2-compatible library with C/C++/Python/Ruby interfaces, geometry error detection, large-design ambition, and automation-friendly integration | fnec-rust has reusable crates, but it still lacks equivalent scripting/binding breadth, geometry guardrails, and automation ergonomics for optimizer-driven workflows | Users building optimizers, services, or research tooling may choose necpp first if fnec-rust is harder to embed or validate | Strengthen fnec-rust as an embeddable automation platform: stable APIs, bindings strategy, and stronger geometry diagnostics |
| 4nec2 | De facto hobbyist workflow standard: broad deck handling, near/far-field views, optimization, sweeps, plotting, Smith/SWR style workflows, report familiarity | Text output contract is still incomplete; GUI/workflow/optimization/reporting are behind; no mature visualization stack yet | Users may accept a solver mismatch sooner than a workflow regression; weak workflow blocks adoption even if impedance is accurate | Treat workflow parity as product work, not a Phase 3 nice-to-have |
| EZNEC | Polished UX, engine abstraction (NEC-2/4/5), strong plots/tables, practical model editing, sweep-oriented operation, production-focused usability | No comparable visualization, project workflow, model editing, or engine-management UX yet; current CLI is accurate but not yet operator-efficient | Without matching usability, fnec-rust remains a developer tool rather than a daily antenna design environment | Pull usability and reporting milestones earlier and make them measurable |
| AutoEZ | Variable-driven studies, optimizer workflows, resonance search, segmentation/convergence studies, matching-network automation, and broad model-format translation around the EZNEC ecosystem | fnec-rust does not yet offer a comparable parameter-study, optimization, convergence-analysis, or matching-network workflow; format translation is also far behind | Users doing serious design-space exploration will keep AutoEZ or equivalent external tooling in the loop even if the core solver is good | Treat AutoEZ as the automation-workflow benchmark and make sweep/optimizer/report automation a first-class roadmap concern |

## Parity gaps and impact

| Gap | Current shortfall | User impact if left unresolved | Parity target | Target phase |
|:----|:------------------|:-------------------------------|:--------------|:-------------|
| PRT-001 | Ground modeling stops well short of NEC-4/EZNEC practical coverage | Verticals, low dipoles, buried conductors, and lossy-earth cases stay untrustworthy | NEC-4-class Sommerfeld and buried/near-ground accuracy for supported models | Phase 2 |
| PRT-002 | Loads, transmission-line/network cards, and source families lag NEC-2/NEC-4 workflows | Many real-world deck files cannot be run or compared without manual simplification | Mainstream NEC-2 workflow parity for loads/TL/networks; NEC-4 current-source subset where practical | Phase 2-3 |
| PRT-003 | Frequency-sweep, gain/pattern, and report output are weaker than 4nec2/EZNEC expectations | Users cannot do normal design iteration, band checks, or pattern review inside one tool | Full text-report parity first, then structured export and plotting parity | Phase 1-2 |
| PRT-004 | GUI and model-workflow maturity is far behind EZNEC/4nec2 | Solver adoption stalls because setup, iteration, and result inspection are slower than incumbent tools | Task-oriented workflow that is faster and clearer than legacy GUIs for common jobs | Phase 3 |
| PRT-005 | No optimization/parameter-sweep workflow comparable to 4nec2 + optimizer, EZNEC + AutoEZ, or library-driven necpp workflows | Design-space search remains manual and external, limiting serious use | Native sweep/optimization hooks with deterministic batch execution | Phase 3-4 |
| PRT-006 | No AutoEZ-class automation for resonance search, convergence studies, matching networks, or variable-driven model generation | Advanced antenna iteration remains slower and more error-prone than incumbent workflows | Automation layer that supports variable sweeps, resonance tools, convergence studies, and matching-network helpers | Phase 3-4 |
| PRT-007 | Geometry validation and embeddability are weaker than necpp-style library use cases | Tooling and service integrations lose time on preventable model mistakes and ad-hoc wrappers | Strong geometry diagnostics, stable APIs, and automation-oriented library surfaces | Phase 2-4 |
| PRT-008 | Accuracy program is strong for the Hallen dipole baseline but not yet broad enough to support "better than NEC" claims | Claims of parity are too narrow; difficult models can regress unnoticed | Corpus extended to ground, loads, sweeps, multi-source, pattern, current, and difficult-geometry sensitivity cases | Phase 1-3 |
| PRT-009 | No NEC-5-class plan for surfaces and difficult geometry robustness | fnec-rust risks plateauing below the modern state of the art | Explicit surfaces strategy and mixed wire/surface roadmap, even if delivered after core wire parity | Phase 4-5 |
| PRT-010 | No explicit validation matrix derived from the NEC-5 Validation Manual test themes (kernel accuracy, source modeling, convergence, surface/junction behavior, loops/ground behavior) | Accuracy claims can miss the exact difficult scenarios NEC-5 uses to expose modeling and convergence weaknesses | Add a documented NEC-5-informed validation matrix and map each covered case to corpus tests with tolerances | Phase 2-3 |
| PRT-011 | No network-distributed authenticated execution mode beyond local CPU threading and GPU offload | Large sweep/optimization workloads cannot scale across trusted LAN or cluster nodes, limiting throughput for serious design-space exploration | Add authenticated distributed solve mode with node discovery, node capability cache, and work-result cache for deterministic repeat runs | Phase 5 (after full GPU support) |

## Phase 0 (done/in place)

- Documentation baseline established under docs/ with YAML frontmatter.
- PR-based last_updated automation path defined for protected main.
- Hallén MoM solver validated: 51-segment λ/2 dipole → 74.24 + j13.90 Ω (matches Python reference).
- Pulse/continuity solver modes marked EXPERIMENTAL (divergence root-caused).
- Core requirement, gap, tolerance matrix, and architectural docs in place.

**Blocker**: BLK-001 (tolerance matrix) — **resolved** 2026-04-22.

## Phase 1 (current focus): NEC foundation and fast progress

**Goals**: Solver breadth, text output contract, golden test corpus, explicit scope guardrails.

**Key deliverables**:
- ✅ Hallén solver working and validated (done).
- ✅ Golden reference corpus expanded and regression-gated: 9 benchmark families including GM/GR equivalence cases.
- ✅ NEC-4 feature boundary documented: card support matrix, phase assignments (done, `docs/nec4-support.md`).
- ✅ Report contract v1 locked in CLI output and CI-gated (`FORMAT_VERSION 1`, deterministic headers/table).
- ✅ Segment current distribution table (`CURRENTS`) landed in CLI report output and contract tests.
- ✅ FR sweep execution landed in CLI and tolerance-gated corpus validation (full multi-point solve path).
- ✅ Hallen topology guardrail landed: non-collinear Hallen requests fail fast with explicit diagnostics instead of returning misleading impedance.
- ✅ GM/GR parser + geometry-builder support landed and is corpus-validated via equivalence decks.
- [x] Expand remaining NEC-2/NEC-4 card execution breadth beyond the current wire/impedance workflow core (RP execution, staged EX family support, TL subset, PT/NT staged handling).
- [x] Assemble golden reference corpus (half-wave dipole free-space/over-ground, Yagi, loaded element, frequency sweep, multi-source, GM/GR equivalence cases).
- [x] Run corpus through reference and fnec-rust; validate all in-scope results within tolerance matrix (CI corpus-validation gate active for contracted in-scope fixtures).
- [x] Simple ground model (infinite, raised dielectric) working and tested (GN type 0 simple finite-ground model activated and regression-gated in corpus CI).
- [ ] CLI-first execution flow complete with all core flags (solver mode, solver options).
- [ ] Produce 4nec2/EZNEC-grade text outputs for impedance, sweep points, gain, pattern, and current tables so the CLI is immediately usable as a daily comparison tool.
- [ ] Close the remaining Phase 1 corpus gaps (loaded element reference parity and broader non-collinear support) so the parity claim is not limited to only the easy cases. Current blocker: Hallen correctly rejects the `dipole-loaded` top-hat geometry as non-collinear, while pulse/continuity/sinusoidal all collapse to the same inaccurate pulse result (`-13.778 + j374.425 Ω` vs external candidate `13.463 - j896.032 Ω` at 7.1 MHz).
- [ ] Ensure the CLI remains at least as scriptable and batch-friendly as open NEC2 tools like yeti01/nec2, including predictable stdin/stdout behavior and stable machine-parseable reporting conventions.

**Blocker dependencies**: BLK-002 (NEC-4 feature boundary).

**Estimated completion**: Q2 2026 (end of April).

## Phase 2: Compatibility expansion and confidence

**Goals**: NEC-4 subset, golden corpus pass, production readiness.

**Key deliverables**:
- [ ] NEC-4 feature boundary clearly defined and documented (BLK-002).
- [ ] NEC-4 subset implemented (e.g., loads, sources, excitation types; exclude: near fields, extended patterns).
- [ ] Golden test corpus corpus passes all fnec-rust solvers within tolerance matrix (BLK-001).
- [ ] CI enforces corpus tolerance pass as a gate on every commit.
- [ ] Advanced ground models added (Sommerfeld, buried antennas).
- [ ] Multi-wire and complex geometry edge cases validated.
- [ ] Pulse/continuity modes fixed via sinusoidal-basis EFIE or remain experimental with clear warnings.
- [ ] Deliver NEC-4-class practical ground behavior for supported cases: near-ground wires, buried conductors where in scope, and lossy-earth validation against reference outputs.
- [ ] Deliver mainstream NEC-2/NEC-4 workflow cards needed by existing 4nec2 and EZNEC users: loads, current-source families, and transmission-line/network subsets.
- [ ] Add pattern, gain, and current corpus validation so accuracy claims extend beyond feedpoint impedance.
- [ ] Add geometry diagnostics and validation guardrails comparable to necpp's automation-oriented error detection so invalid or fragile models fail early with actionable messages.
- [ ] Add a NEC-5 validation-manual coverage matrix for Phase 2 scope, explicitly mapping kernel, source, convergence, and ground/loop classes to reproducible corpus cases.

**Estimated completion**: Q3 2026 (end of July).

## Phase 3: UX and workflow productization

**Goals**: GUI, project workflows, user experience polish.

**Key deliverables**:
- [ ] Modern, intuitive task-oriented GUI on iced.
- [ ] GUI/CLI behavior parity for core tasks (deck parse, solve, report).
- [ ] Project-oriented workflows (import, export, run history, result storage).
- [ ] Plugin API design and safety model baseline (BLK-004).
- [ ] Comprehensive contributor and user documentation.
- [ ] Match or exceed 4nec2/EZNEC usability for common tasks: deck editing, sweep setup, result browsing, pattern slicing, Smith/SWR style inspection, and repeat-run ergonomics.
- [ ] Add native parameter sweep and optimization workflows so users do not need external wrappers for normal iterative design work.
- [ ] Provide an open-source workbench story competitive with xnec2c on Linux: integrated deck editing, fast rerun loops, and direct graphical inspection of sweep and field results.
- [ ] Add AutoEZ-class automation primitives: variable sweeps, resonance targeting, segmentation/convergence studies, and matching-network workflow helpers.
- [ ] Ensure external optimizer-loop interoperability comparable to xnec2c-optimize, including deterministic objective evaluation runs and stable machine-readable outputs.

**Estimated completion**: Q4 2026 (end of October).

## Phase 4: Extensibility

**Goals**: Plugin system, scripting hooks, community extensions.

**Key deliverables**:
- [ ] Plugin/scripting architecture designed and documented.
- [ ] First stable extension points (deck post-processors, result filters, custom reports).
- [ ] Sandboxing model and dependency policy (BLK-005, GPLv2 compatibility).
- [ ] Plugin registry and distribution mechanism.
- [ ] Provide scripting and automation hooks comparable to the practical value users get today from AutoEZ-style and optimizer-assisted workflows.
- [ ] Publish a stable automation surface for batch studies, optimizer loops, and custom result extraction.
- [ ] Define bindings and embedding strategy for non-Rust consumers so fnec-rust can compete with necpp in optimization and research pipelines.
- [ ] Support automation-driven model transformation workflows comparable in value to AutoEZ's variable substitution and repeated-study orchestration, without requiring spreadsheet tooling.
- [ ] Define distributed authenticated execution architecture handoff requirements (transport, authN/authZ, worker contract, failure semantics) to be activated only after full GPU solver support is complete.

**Estimated completion**: Q1 2027 (end of March).

## Phase 5: Performance scaling

**Goals**: GPU acceleration from postprocessing to solver kernel.

### Benchmark mode matrix (all targets)

To prevent regressions and keep performance claims comparable, benchmarking must run in three modes on every supported target class:

- **CPU single-threaded**: deterministic baseline (`RAYON_NUM_THREADS=1` or equivalent) for direct algorithmic comparison.
- **CPU multithreaded**: throughput mode using target-default thread count and an explicit fixed-thread variant for reproducibility.
- **GPU offload**: accelerator path (where available) with framework/runtime metadata captured in benchmark artifacts.

Target classes for this matrix:

- Desktop/workstation (`x86_64` Linux, Windows, macOS)
- SBC/edge (`aarch64` Raspberry Pi class)
- Remote worker/cluster nodes used by distributed execution mode (post-full-GPU milestone)

Required benchmark outputs per target/mode:

- wall-clock runtime, solver/kernel time split, and memory footprint
- problem-size metadata (segments, wires, frequency points, solver mode)
- run metadata (CPU model, GPU model, driver/runtime, thread count)
- regression deltas vs last accepted baseline

**Key deliverables**:
- [ ] GPU acceleration for postprocessing (pattern interpolation, report generation).
- [ ] Benchmark CPU vs GPU behavior; define selection criteria.
- [ ] Plan staged expansion to matrix fill and solve on AMD ROCm / OpenCL / SYCL (DEC-008).
- [ ] Prototype GPU solver kernel on reference geometry.
- [ ] Complete full GPU solver support (matrix fill + solve path) as the explicit prerequisite before any distributed/network clustering implementation begins.
- [ ] CI benchmarking dashboard for performance tracking.
- [ ] Define the post-NEC-4 accuracy frontier: surfaces, mixed wire/surface problems, and difficult-geometry sensitivity regression tracking informed by NEC-5-class expectations.
- [ ] Establish whether fnec-rust will pursue NEC-5-class mixed-potential and surface capability directly or via an explicitly documented alternative architecture.
- [ ] Deliver cluster execution mode beyond multithreading/GPU only after full GPU support is complete: authenticated node discovery, cached node capability inventory, and work-content/result caching to reduce repeated transfer/compute costs across distributed runs.

**Estimated completion**: Q2 2027 (end of June).

## Gaps and blockers (from requirements.md)

| Gap | Title | Priority | Target | Owner | Resolution criteria |
|:---|:------|:---------|:-------|:------|:-------------------|
| GAP-002 | NEC-4 feature boundary | **CRITICAL** | Phase 2 end | (TBD) | BLK-002: Explicit list of supported/deferred NEC-4 cards/features in docs/nec4-support.md |
| GAP-003 | MVP ground model set | **HIGH** | Phase 1 end | (TBD) | Simple (infinite, raised dielectric) implemented; advanced (Sommerfeld, buried) in Phase 2 plan |
| GAP-004 | Plugin/scripting interface | **HIGH** | Phase 3 end | (TBD) | BLK-004: API design, safety model, first two extension points documented and working |
| GAP-005 | 4nec2-like text report format | **HIGH** | Phase 1 end | CLI+Reporting | **Resolved 2026-04-23** via PAR-001 v1 contract and CI gate |
| GAP-006 | GUI information architecture | **MEDIUM** | Phase 3 end | (TBD) | IA document, wireframes, and user testing feedback collected |
| GAP-007 | GPU rollout criteria | **MEDIUM** | Phase 5 end | (TBD) | Framework selection follows DEC-008 (FOSS-first, AMD-preferred); benchmarks published |
| GAP-008 | Dependency/license policy | **MEDIUM** | Phase 2 end | (TBD) | BLK-005: Policy thresholds, exception process, GPLv2 compatibility rules documented |

## Competitive parity work items

| Item | Baseline comparator | Priority | Needed for parity because | Planned resolution |
|:-----|:--------------------|:---------|:--------------------------|:-------------------|
| CP-001 | NEC-2 / 4nec2 / EZNEC | **CRITICAL** | Feedpoint-only parity is too narrow; mainstream users expect sweep, gain, pattern, and current agreement too | Extend corpus and report contract in Phase 1-2 |
| CP-002 | NEC-4 / EZNEC | **CRITICAL** | Low-height, lossy-ground, and buried-conductor work is a core production use case, not an edge case | Prioritize advanced ground in Phase 2 |
| CP-003 | NEC-2 / NEC-4 / 4nec2 | **HIGH** | Missing loads, TL, network, and source cards break existing deck portability | Implement mainstream missing cards in Phase 2-3 |
| CP-004 | 4nec2 / EZNEC | **HIGH** | Weak visualization and workflow will block adoption even if solver accuracy is strong | Pull workflow parity into Phase 3 with explicit acceptance criteria |
| CP-005 | xnec2c / 4nec2 / EZNEC | **HIGH** | Open-source and commercial incumbents already offer integrated inspection workflows that make iteration faster than raw CLI runs | Add graphical and report-driven workflow parity in Phase 3 |
| CP-006 | AutoEZ / necpp | **HIGH** | Real antenna design depends on sweeps, optimization, resonance targeting, convergence studies, and embedding into automation pipelines, not one-off runs | Add deterministic sweep/optimization support plus stable automation APIs in Phase 3-4 |
| CP-007 | yeti01/nec2 / xnec2c / necpp | **MEDIUM** | Open-source alternatives already cover clean batch execution, reference behavior, or embeddable integration from different angles | Keep fnec-rust competitive on CLI stability, library ergonomics, and diagnostics, not just GUI ambition |
| CP-008 | AutoEZ | **MEDIUM** | Matching-network helpers, variable-driven model generation, and format-translation workflows materially reduce engineering friction today | Plan explicit automation helper features instead of assuming generic scripting will cover them; defer purchase until a Phase 3 workflow benchmark needs hands-on verification |
| CP-009 | NEC-5 | **MEDIUM** | NEC-5-class robustness is the longer-term path to claiming "better than NEC-2/4" instead of just "compatible with" | Create explicit Phase 5 architecture decision on surfaces and mixed-potential methods |
| CP-010 | xnec2c-optimize | **MEDIUM** | Open-source users already have a practical optimizer loop with repeatable objective-driven tuning workflows | Add optimizer-loop compatibility criteria (CLI/API contracts, objective I/O stability, convergence-study support) in Phase 3-4 |
| CP-011 | HPC scheduler + cluster workflows | **MEDIUM** | Serious sweep and optimization studies need distributed execution, trust boundaries, and repeat-run caching that local-only acceleration cannot provide | Add authenticated distributed mode with discovery and caching after full GPU support (late Phase 5+), with SSH-backed deployment path first |
| CP-012 | 4nec2 external kernel binary workflow | **LOW (deferred)** | Users can keep mature 4nec2 UX while swapping in a faster kernel only if invocation compatibility is preserved | Defer to Phase 4-5: complete filename-steered drop-in mode, Windows replacement workflow compatibility checks, and representative 4nec2 kernel-call contract validation |

## Commercial benchmark acquisition policy

- **Do not purchase AutoEZ yet.** The current Phase 1-2 work can be benchmarked adequately with public documentation, 4nec2/EZNEC behavior descriptions, xnec2c, open NEC2 references, and explicit workflow-gap analysis.
- Revisit AutoEZ acquisition when Phase 3 work begins on automation parity: variable sweeps, resonance targeting, convergence studies, matching-network helpers, and repeated-analysis orchestration.
- Purchase becomes justified when hands-on validation is needed for UX details that public documentation cannot settle reliably, especially around multi-step study workflows and format-translation behavior.
- Until then, treat AutoEZ as a design benchmark and workflow target, not as a required reference engine for numerical validation.

## Gap-driven milestone blockers (Phase gates)

| Blocker | Resolution condition | Gate |
|:--------|:---------------------|:-----|
| BLK-001 | Tolerance matrix defined with metrics for R, X, gain, pattern, current, phase | Phase 1 → Phase 2 |
| BLK-002 | NEC-4 feature boundary documented (supported/deferred cards/sources); see [docs/nec4-support.md](docs/nec4-support.md) | Phase 1 → Phase 2 |
| BLK-003 | 4nec2 text report format contract locked; golden corpus results validated within tolerance | **Resolved 2026-04-23** |
| BLK-004 | Plugin API design, safety model, first two extension points working | Phase 3 → Phase 4 |
| BLK-005 | GPLv2 dependency policy thresholds and exception process documented | Phase 2 → Phase 3 |

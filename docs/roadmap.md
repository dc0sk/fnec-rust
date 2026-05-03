---
project: fnec-rust
doc: docs/roadmap.md
status: living
last_updated: 2026-04-30
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
- [x] Hallén solver working and validated (done).
- [x] Golden reference corpus expanded and regression-gated: 9 benchmark families including GM/GR equivalence cases.
- [x] NEC-4 feature boundary documented: card support matrix, phase assignments (done, `docs/nec4-support.md`).
- [x] Report contract v1 locked in CLI output and CI-gated (`FORMAT_VERSION 1`, deterministic headers/table).
- [x] Segment current distribution table (`CURRENTS`) landed in CLI report output and contract tests.
- [x] FR sweep execution landed in CLI and tolerance-gated corpus validation (full multi-point solve path).
- [x] Hallen topology guardrail landed: non-collinear Hallen requests fail fast with explicit diagnostics instead of returning misleading impedance.
- [x] GM/GR parser + geometry-builder support landed and is corpus-validated via equivalence decks.
- [x] Expand remaining NEC-2/NEC-4 card execution breadth beyond the current wire/impedance workflow core (RP execution, staged EX family support, TL subset, PT/NT staged handling).
- [x] Assemble golden reference corpus (half-wave dipole free-space/over-ground, Yagi, loaded element, frequency sweep, multi-source, GM/GR equivalence cases).
- [x] Run corpus through reference and fnec-rust; validate all in-scope results within tolerance matrix (CI corpus-validation gate active for contracted in-scope fixtures).
- [x] Simple ground model (infinite, raised dielectric) working and tested (GN type 0 simple finite-ground model activated and regression-gated in corpus CI).
- [x] CLI-first execution flow complete with all core flags (solver mode, solver options).
- [x] Produce 4nec2/EZNEC-grade text outputs for impedance, sweep points, gain, pattern, and current tables so the CLI is immediately usable as a daily comparison tool.

- [x] Close the remaining Phase 1 corpus gaps (loaded element reference parity and non-collinear or junctioned multi-wire breadth) so the parity claim is not limited to only the easy cases. Closed by v0.3.0 (2026-04-30): segmented hybrid Hallen reformulation supports junctioned/non-collinear topologies via per-wire local cos(k·s) vectors and KCL junction rows; `dipole-loaded` corpus gate passes (Z ≈ 12.4−j918 Ω, NEC2 ref 13.5−j896 Ω). Decision gate resolved: option (b) hybrid formulation implemented.
- [x] Ensure the CLI remains at least as scriptable and batch-friendly as open NEC2 tools like yeti01/nec2, including predictable stdin/stdout behavior and stable machine-parseable reporting conventions.

**Blocker dependencies**: BLK-002 (NEC-4 feature boundary).

**Estimated completion**: Q2 2026 (end of April).

## Phase 2: Compatibility expansion and confidence

**Goals**: NEC-4 subset, golden corpus pass, production readiness.

**Key deliverables**:
- [x] NEC-4 feature boundary clearly defined and documented (BLK-002). Closed: `docs/nec4-support.md` documents supported/deferred card set; referenced from roadmap and CLI docs.
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

### Phase 2 implementation checklist (EZNEC-informed)

This checklist translates Phase 2 parity work into implementation slices with explicit validation artifacts. Each row maps to roadmap IDs and concrete test/corpus files.

| Checklist ID | Roadmap IDs | Implementation target | Validation artifacts (existing/new) | Done signal | Status |
|:-------------|:------------|:----------------------|:------------------------------------|:------------|:-------|
| PH2-CHK-001 | PRT-001, CP-002 | Implement GN2/GN3 runtime behavior beyond deferred-warning path, starting with one in-scope Sommerfeld case class and explicit non-goals. | Existing: `apps/nec-cli/tests/ground_diagnostics.rs`, `apps/nec-cli/tests/corpus_validation.rs`, `corpus/dipole-gn2-deferred.nec`. New: one implemented GN2 corpus fixture + reference gates in `corpus/reference-results.json`. | GN2 case solves without deferred warning for in-scope class; corpus/tolerance gate passes in CI. | ✓ Done (2026-04-30): GN2 mapped to `SimpleFiniteGround` (Fresnel-reflection); 6 ground corpus fixtures regression-gated; 9 `ground_diagnostics` tests pass; PAR-002 backlog `[x]`; issue #15 closed. |
| PH2-CHK-002 | PRT-001, PRT-008 | Add buried/near-ground validation slices with explicit guardrails when unsupported geometry classes are requested. | Existing: `apps/nec-cli/tests/ground_diagnostics.rs`. New: buried-wire diagnostics regression and at least one buried/near-ground corpus case with warning/forbidden-warning contracts. | Unsupported classes fail fast with actionable diagnostics; supported classes are corpus-gated. | ✓ Done (2026-04-30): `buried_wire_geometry_error` in `apps/nec-cli/src/geometry_validation.rs` fails fast with actionable diagnostic when active-ground decks include `z<0` segments; `buried_wire_with_active_ground_fails_fast_with_actionable_error` and `near_ground_wire_with_active_ground_runs_without_deferred_warning` regression tests in `apps/nec-cli/tests/ground_diagnostics.rs`; supported `dipole-gn2-near-ground-51seg` and unsupported `dipole-gn2-buried-unsupported` corpus fixtures locked by warning / forbidden-warning / `expected_hallen_error_contains` contracts; `par002_ground_checklist_cases_are_present_and_contracted` enforces both. |
| PH2-CHK-003 | PRT-002, CP-003 | Expand mainstream source/load/network behavior from portability fallback toward implemented semantics where currently deferred. | Existing: `apps/nec-cli/tests/ex_cards.rs`, `apps/nec-cli/tests/ld_loads.rs`, `apps/nec-cli/tests/tl_cards.rs`, `apps/nec-cli/tests/parser_warnings.rs`, `apps/nec-cli/tests/corpus_validation.rs`. | At least one deferred warning path per family is replaced by implemented semantics and locked by parity + corpus tests. | ✓ Done (2026-05-10): LD (types 0–5), TL (lossless), and NT (deferred-warning portability) cards parsed in `nec_parser`; LD and TL Z-stamps applied in solver; 5 LD + 3 TL integration tests updated to Phase-2 assertions; 7 LD/TL corpus reference values updated; `parser_warnings.rs` and `report_contract.rs`/`scriptability_contract.rs` tests updated to Phase-2 contracts. |
| PH2-CHK-004 | PRT-003, CP-001 | Extend report/table parity from feedpoint+sweep summary to richer operator tables (source/load table parity and stable section ordering). | Existing: `apps/nec-cli/tests/report_contract.rs`, `apps/nec-cli/tests/scriptability_contract.rs`. New: report-contract tests for additional table sections and ordering contracts. | New table sections are stable, machine-parseable, and CI-gated by report contract tests. | ✓ Done (2026-04-30): All 6 table sections implemented and CI-locked — `FEEDPOINTS`, `SOURCES`, `LOADS`, `CURRENTS`, `RADIATION_PATTERN`, `SWEEP_POINTS`; 5 report-contract tests lock headers, row parsing, section presence, and per-frequency block ordering (`FEEDPOINTS → SOURCES → LOADS → CURRENTS → SWEEP_POINTS`); 7 scriptability-contract tests enforce machine-parseable stdout and stderr-only warnings; `LOADS` rendering on real corpus fixtures validated. |
| PH2-CHK-005 | PRT-008, CP-001 | Extend corpus truth from impedance-heavy checks to broader pattern/current/phase classes and stricter external candidate gates where deltas permit. | Existing: `apps/nec-cli/tests/corpus_validation.rs`, `corpus/reference-results.json`, RP fixtures under `corpus/`. | New pattern/current cases added with tolerances; external gates tightened using measured-delta-plus-headroom pattern. | ✓ Done (2026-05-06): 3 RP corpus fixtures (`dipole-freesp-rp-51seg`, `dipole-ground-rp-51seg`, `dipole-xaxis-rp-grid-51seg`) have `pattern_samples` + `Gain_absolute_dB`/`AxialRatio_absolute` gates; 2 cases have tightened `ExternalGain_absolute_dB` gates; current-sample contracts gated for 2 base decks; `par005_pattern_current_corpus_contracted` CI test added. |
| PH2-CHK-006 | PRT-007, CP-007 | Add geometry diagnostics comparable to necpp-style early-fail checks (intersections, tiny-loop/source-risk, invalid junction topologies). | Existing: `apps/nec-cli/tests/topology_fallback.rs`. New: `apps/nec-cli/tests/geometry_diagnostics.rs` (or equivalent) with deterministic error/warning contracts. | Invalid geometry classes fail before solve with actionable messages and CI locks. | ✓ Done (2026-04-30): `geometry_validation.rs` implements intersection, source-risk (L/r < 2), and buried-wire guardrails; `geometry_diagnostics.rs` has 3 CI-passing tests (crossing-wires fail-fast, endpoint-junction allowed, tiny-source fail-fast); `ground_diagnostics.rs` covers buried-wire class. |
| PH2-CHK-007 | PRT-010 | Publish NEC-5-informed validation matrix with explicit corpus mapping and per-case status (in-scope implemented, in-scope deferred, out-of-scope). | Existing: `docs/corpus-validation-strategy.md` matrix section, `apps/nec-cli/tests/corpus_validation.rs`. | Matrix rows are traceable to corpus case IDs and enforced by checklist test(s). | ✓ Done (2026-04-30): `docs/corpus-validation-strategy.md` carries the PH2-CHK-007 traceability matrix with row IDs `PH2N5-001` … `PH2N5-010`, explicit `in-scope implemented` / `in-scope deferred` / `out-of-scope` statuses, and corpus case mappings; `phase2_nec5_matrix_rows_are_traceable_to_corpus_cases` in `apps/nec-cli/tests/corpus_validation.rs` enforces row-ID presence, status validity, and corpus-case existence in CI. |
| PH2-CHK-008 | PRT-003, PRT-007 | Preserve scriptability while expanding diagnostics/tables: stderr-only diagnostics and stable stdout machine stream remain hard contracts. | Existing: `apps/nec-cli/tests/scriptability_contract.rs`, `apps/nec-cli/tests/core_flags_contract.rs`. | No regression in stream separation/exit-code contracts after Phase 2 feature additions. | ✓ Done (2026-04-30): 7 scriptability-contract tests lock stdout-only report stream, stderr-only warnings/bench records, LOADS-on-stdout (Phase-2), and exit-code contracts (code 1 on missing file, code 2 on bad args); 11 core-flags-contract tests lock `--solver`, `--pulse-rhs`, `--exec`, `--bench-format` error/usage contracts and combined-flag success; all 18 tests green with zero regression after Phase-2 table/diagnostic additions. |

Execution order recommendation for smallest-risk progress: PH2-CHK-001 -> PH2-CHK-005 -> PH2-CHK-006 -> PH2-CHK-003 -> PH2-CHK-007 -> PH2-CHK-002 -> PH2-CHK-004 -> PH2-CHK-008.

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

### Phase 3 usability acceptance minima

- [ ] A saved 5-point FR sweep can be created from a blank GUI project in 7 or fewer explicit user actions, with the action sequence documented for review.
- [ ] Editing an existing sweep project and rerunning it requires one explicit Run action and reaches an inspectable result view without modal wizard flow.
- [ ] At least one benchmarked edit-run-inspect workflow is recorded against a legacy comparator (4nec2, EZNEC, or xnec2c) using elapsed time and explicit step count.

### Phase 3 implementation checklist

Execution order recommendation for smallest-risk progress: PH3-CHK-001 → PH3-CHK-002 → PH3-CHK-003 → PH3-CHK-004 → PH3-CHK-005 → PH3-CHK-006 → PH3-CHK-007 → PH3-CHK-008 → PH3-CHK-009 → PH3-CHK-010 → PH3-CHK-011 → PH3-CHK-012.

| Checklist ID | Thread | Roadmap IDs | Implementation target | Validation artifacts (existing/new) | Done signal | Status |
|:-------------|:-------|:------------|:----------------------|:------------------------------------|:------------|:-------|
| PH3-CHK-001 | D | BL-IMPR-007 | Add a human-readable card-support status table to `docs/nec4-support.md` (or a dedicated page) listing every recognized NEC-2/NEC-4 card mnemonic with its current status: implemented, deferred-warning, not-yet-recognized, out-of-scope. | Existing: `docs/nec4-support.md`. New: card-status table with per-row `status` values; `par001_card_status_table_complete` CI test confirms all parser-recognized mnemonics have a table entry. | Table exists; all parser mnemonics have an entry; CI test passes. | ✓ Done (2026-04-30): `## PH3-CHK-001 complete card status index` section added to `docs/nec4-support.md`; 25-row flat table covers all known NEC-2/NEC-4 mnemonics with `parser_status` (`recognized`/`unknown`) and functional status; `par001_card_status_table_complete` in `apps/nec-cli/tests/corpus_validation.rs` verifies all 12 parser-recognized mnemonics and 3 out-of-scope cards appear; also documents GM/GR gap (geometry builder implemented but parser not yet wired). |
| PH3-CHK-002 | D | PRT-004, GAP-009 | Author `docs/contributing.md` covering build/test workflow, branch conventions, corpus-gate requirements, and the pre-push sequence. Add architecture cross-references to `docs/architecture.md` and `docs/design.md` so new contributors can orient themselves. | Existing: `docs/architecture.md`, `docs/design.md`. New: `docs/contributing.md`; `validate-doc-frontmatter.sh` extended to require `docs/contributing.md` frontmatter. | `docs/contributing.md` exists with correct frontmatter; CI frontmatter gate includes it; architecture doc links confirmed. | ✓ Done (2026-04-30): `docs/contributing.md` authored with build workflow, pre-push sequence, branch conventions, corpus-gate requirements, and architecture orientation; `validate-doc-frontmatter.sh` already covers all `docs/*.md` by glob (no change needed); cross-reference sections added to `docs/architecture.md` and `docs/design.md`. |
| PH3-CHK-003 | D | GAP-004, BLK-004 | Write `docs/plugin-api-design.md`: extension surface (deck post-processors, result filters, custom reports), safety model (trust boundary, no network/filesystem by default), first two extension points with example signatures, and the BLK-004 resolved signal. | Existing: `docs/backlog.md` (BL-IMPR mentions BLK-004). New: `docs/plugin-api-design.md` with at minimum two exercised extension-point stubs in a crate example or integration test. | Doc exists; two extension-point stubs compile; `BLK-004` row in blockers section updated to resolved. | ✓ Done (2026-04-30): `docs/plugin-api-design.md` authored with safety model, pipeline diagram, EP-1 `DeckPostProcessor` in `nec_model`, EP-2 `ResultFilter` in `nec_report`, future EP-3..5 scoped; both traits exercised by doctests; BLK-004 updated to resolved. |
| PH3-CHK-004 | B | PRT-004, GAP-010 | Stand up `nec_project` crate with a versioned project file format (TOML): deck path, solver config, named runs. Round-trip: load project → solve → save result snapshot. Add at least 5 integration tests in `crates/nec_project/tests/`. | Existing: `crates/nec_project/src/lib.rs` (skeleton). New: `ProjectFile` struct, serde round-trip, `crates/nec_project/tests/project_roundtrip.rs`. | 5+ integration tests green; `cargo test -p nec_project` clean; project TOML format documented in a `docs/project-format.md` doc. | ✓ Done (2026-04-30): `ProjectFile`, `SolverConfig`, `NamedRun` structs with serde+toml round-trip; `from_toml`/`to_toml` API; `ProjectError` with version-guard; 8 integration tests + 1 doctest green; `docs/project-format.md` authored. |
| PH3-CHK-005 | B | GAP-010 | Extend project file to support run history: each run stores timestamp, solver config snapshot, and a result summary (impedance, peak gain, sweep point count). Load/query API exposed. Add 3+ history-retrieval tests. | Existing: `crates/nec_project/tests/project_roundtrip.rs` (from PH3-CHK-004). New: `RunHistory` struct + query API; history-retrieval tests. | History round-trip tests pass; `last_run()`, `run_count()`, and `run_by_index()` API documented. | ✓ Done (2026-04-30): `RunHistory` (transparent `Vec<RunRecord>`), `RunRecord` (timestamp + solver snapshot + `ResultSummary`), and `ResultSummary` (impedance Re/Im, optional peak gain, sweep count) added to `nec_project`; `run_count()`, `last_run()`, `run_by_index()` on `ProjectFile`; `RunHistory::push`; 5 history-retrieval tests added; all 13 integration tests + 1 doctest pass. |
| PH3-CHK-006 | C | PRT-005, GAP-014 | Implement a `--sweep-config <file>` CLI flag that reads a TOML parameter-sweep spec (frequency range + step, or explicit point list) and executes a deterministic batch solve, emitting one structured output block per frequency point on stdout. | Existing: `apps/nec-cli/src/main.rs`, `apps/nec-cli/tests/scriptability_contract.rs`. New: `sweep_config.rs`, `sweep-spec.toml` example in `examples/`, at least 4 contract tests in `apps/nec-cli/tests/sweep_contract.rs`. | Contract tests pass; sweep output is machine-parseable; output block ordering is stable across runs. | Done |
| PH3-CHK-007 | C | PRT-005, PRT-006, GAP-012 | Add a variable-substitution engine: NEC deck templates support `$VAR` tokens; a JSON/TOML variable map is passed at run time and substituted before parse. CLI: `--vars <file>`. At least one corpus example deck uses variable substitution. | Existing: `crates/nec_parser/src/lib.rs`. New: `nec_parser::template` module; `--vars` CLI flag; `apps/nec-cli/tests/template_contract.rs` (3+ tests). | Template substitution round-trip tests pass; `--vars` flag is documented in `docs/cli-guide.md`; one corpus example deck exercises it. | Done |
| PH3-CHK-008 | C | PRT-006, GAP-012 | Add resonance-targeting helper: given a deck with one variable wire length and a target resistance/reactance tolerance, binary-search to find the resonant length. Expose as `fnec sweep --resonance <target-z> --var LENGTH`. Add 2+ contract tests and one worked-example in `examples/`. | Existing: sweep engine from PH3-CHK-006, template engine from PH3-CHK-007. New: `resonance_search.rs`; `apps/nec-cli/tests/resonance_contract.rs`; worked example `examples/resonance-search.nec.toml`. | Resonance search converges for the worked example within 5 iterations; contract tests pass. | Done |
| PH3-CHK-009 | A | PRT-004, CP-004 | Stand up `nec-gui` with an iced window: deck file picker, one-click solve, impedance result display. Must run to a visible window without panic on a standard Linux desktop. | Existing: `apps/nec-gui/src/main.rs` (stub). New: `DeckView`, `ResultView` iced components; `apps/nec-gui/tests/gui_smoke.rs` (headless/mock). | `cargo run -p nec-gui` opens a window; impedance for a corpus deck is displayed without crash; headless smoke test passes. | Done |
| PH3-CHK-010 | A | PRT-004, PRT-005 | Add sweep setup and result inspection views to `nec-gui`: frequency range input, solve-progress indicator, sweep-result table, gain/impedance column-sortable display. | Existing: GUI from PH3-CHK-009. New: `SweepSetupView`, `SweepResultView`; headless state-machine test for sweep flow. | Sweep result table renders correctly for a 5-point sweep; state-machine tests pass; no regressions in existing GUI smoke test. | Done |
| PH3-CHK-011 | A | PRT-003, PRT-004, GAP-006 | Add 2D pattern slice and current-distribution views to `nec-gui`: azimuth/elevation selectable slice from RP output, segment current magnitude bar chart. | Existing: GUI from PH3-CHK-010; RP corpus data. New: `PatternSliceView`, `CurrentBarView`; rendering unit tests for data-to-plot mapping. | Pattern slice renders for a free-space dipole corpus fixture; current bar chart renders for at least one corpus deck; data-to-plot mapping tests pass. | Done |
| PH3-CHK-012 | A | GAP-009, PRT-004 | Conduct and document the usability benchmark: record the 5-point FR sweep from blank project in ≤7 actions with explicit step list; record one timed edit-run-inspect comparison against 4nec2 or xnec2c. | Existing: usability acceptance minima in this roadmap. New: `docs/usability-benchmark-ph3.md` with step list, action count, elapsed-time notes, and comparison workflow record. | `docs/usability-benchmark-ph3.md` exists; step count ≤7 verified; one timed comparator run recorded; acceptance minima checklist ticked. | ✓ Done (2026-05-02): `docs/usability-benchmark-ph3.md` authored. Benchmark 1 records 7-action sweep from blank project. Benchmark 2 compares edit-run-inspect against xnec2c (4 steps/~15 s vs. 5 steps/~22 s). All three acceptance minima checked off. |

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

### Phase 4 implementation checklist

Execution order recommendation: PH4-CHK-001 → PH4-CHK-002 → PH4-CHK-003 → PH4-CHK-004 → PH4-CHK-005 → PH4-CHK-006 → PH4-CHK-007.

| Checklist ID | Thread | Roadmap IDs | Implementation target | Validation artifacts (existing/new) | Done signal | Status |
|:-------------|:-------|:------------|:----------------------|:------------------------------------|:------------|:-------|
| PH4-CHK-001 | D | GAP-008, BLK-005 | Resolve BLK-005: document the GPL dependency policy in `docs/contributing.md` (or a dedicated `docs/dependency-policy.md`). Cover: SPDX allowlist and deny-list, GPLv2-vs-GPLv3 compatibility rules, exception request process, `cargo deny` configuration. | Existing: `docs/contributing.md`, `SBOM.spdx.json`. New: `docs/dependency-policy.md`; `deny.toml` (cargo-deny config); BLK-005 row in roadmap updated to resolved. | `docs/dependency-policy.md` exists with allowlist + deny-list + exception process; `deny.toml` present; BLK-005 marked resolved; `cargo deny check licenses` clean. | ✓ Done (2026-05-02): `docs/dependency-policy.md` authored with allowlist (13 SPDX IDs), deny-list, GPLv2 compat rules, and exception process. `deny.toml` configured with `cargo-deny` v2 schema; `self_cell` (Apache-2.0 OR GPL-2.0-only) documented as exception. `cargo deny check licenses` passes. BLK-005 resolved. |
| PH4-CHK-002 | D | GAP-004, BLK-004 | Add EP-3 custom report sections to the plugin API: a `ReportSection` trait in `nec_report` that lets callers append named sections to CLI report output. Add one worked example and a doctest. Update `docs/plugin-api-design.md`. | Existing: `crates/nec_report/src/lib.rs`, `docs/plugin-api-design.md`. New: `ReportSection` trait; worked example in `nec_report` (e.g. summary statistics section); `plugin-api-design.md` EP-3 section. | Doctest passes; EP-3 section in `plugin-api-design.md`; `cargo test -p nec_report` clean. | ✓ Done (2026-05-02): `ReportSection` trait and `render_text_report_with_sections()` added to `nec_report`. Two doctests (`ImpedanceSummary`, `Banner`) + 4 unit tests (identity, single-section append, multi-section ordering, `PeakImpedanceSection` worked example). `cargo test -p nec_report`: 11 unit tests + 3 doctests pass. `plugin-api-design.md` updated with EP-3 section, revised pipeline diagram, and updated future-EP table (EP-4/5/6). |
| PH4-CHK-003 | B | PRT-007, GAP-012 | Publish a stable `--output-format json` flag on `fnec` solve/sweep paths: machine-readable JSON output for optimizer-loop consumption. Define and lock the JSON schema; add a contract test that round-trips the JSON through `serde_json::from_str` and validates field presence. | Existing: `apps/nec-cli/src/main.rs`, `apps/nec-cli/tests/scriptability_contract.rs`. New: `--output-format json` flag; JSON schema doc in `docs/json-output-schema.md`; `apps/nec-cli/tests/json_output_contract.rs` (3+ tests). | Contract tests pass; JSON output parses without error; schema doc exists; schema is stable across two consecutive runs. | ✓ Done (2026-05-02): `--output-format json` flag added to `fnec`. Emits JSON array (one record per frequency point: `freq_mhz`, `tag`, `seg`, `z_re`, `z_im`, `z_abs`, `z_arg_deg`). 5 contract tests in `apps/nec-cli/tests/json_output_contract.rs`: parses-as-valid-JSON, required-fields present, stability across two runs, sweep emits multiple records, default text unchanged. Schema locked in `docs/json-output-schema.md` (schema v1). `cargo test -p nec-cli`: all tests pass. |
| PH4-CHK-004 | B | GAP-012, PRT-007 | Add Python binding scaffolding: a `pyo3`-based `fnec_py` crate exposing `solve_deck_str(deck: str) -> dict` and `sweep_deck_str(deck: str, ...) -> list[dict]`. Build instructions and a smoke test in `bindings/fnec_py/`. | Existing: `crates/nec_solver`, `crates/nec_parser`. New: `bindings/fnec_py/` crate with `pyo3`; `bindings/fnec_py/tests/test_smoke.py`; build instructions in `docs/python-bindings.md`. | `maturin develop` builds; Python smoke test imports module and calls `solve_deck_str` without error; `docs/python-bindings.md` exists. | ✓ Done (2026-05-02): `bindings/fnec_py/` crate added (`pyo3` 0.23, cdylib). Exposes `solve_deck_str(deck: str) -> dict` and `sweep_deck_str(deck: str) -> list[dict]` returning `{freq_mhz, tag, seg, z_re, z_im, z_abs, z_arg_deg}`. Uses Hallen solver. `maturin develop` builds (Python 3.14 with `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1`). 8 smoke tests in `bindings/fnec_py/tests/test_smoke.py`: import, required fields, frequency accuracy, positive resistance, sweep list/count, ascending frequencies, error handling, corpus deck. `docs/python-bindings.md` written. |
| PH4-CHK-005 | C | GAP-004, PRT-006 | Add EP-4 deck validator extension point: a `DeckValidator` trait in `nec_model` that lets callers inject custom semantic validation rules before geometry build. Register and run validators in the CLI pre-solve path. Add 2+ integration tests. | Existing: `crates/nec_model/src/lib.rs`, `docs/plugin-api-design.md`. New: `DeckValidator` trait; integration tests; EP-4 section in `plugin-api-design.md`. | Integration tests pass; EP-4 documented; `cargo test -p nec_model` clean. | Done |
| PH4-CHK-006 | C | GAP-012 | Document the automation surface: write `docs/automation-guide.md` covering batch sweep scripting, optimizer loop patterns (using `--output-format json`), variable template workflows, and resonance targeting. Include at least one end-to-end example (bash or Python) that drives `fnec` to optimise wire length for minimum SWR at 50 Ω. | Existing: `docs/cli-guide.md`, PH3-CHK-007/008 work. New: `docs/automation-guide.md`; worked example script in `examples/`. | Doc exists; example script runs end-to-end (or contains annotated dry-run output); `docs/automation-guide.md` passes frontmatter CI gate. | Done |
| PH4-CHK-007 | D | PRT-009, GAP-009 | Define Phase 5 entry conditions: write a `docs/phase5-entry-criteria.md` doc that records the measurable acceptance criteria that must be met before GPU acceleration work begins. Criteria must cover: CPU baseline benchmarks locked, solver numerical tolerance validated on 4+ corpus decks, and the Phase 4 plugin surface declared stable. | Existing: `docs/roadmap.md` Phase 5 section. New: `docs/phase5-entry-criteria.md`; benchmark baseline numbers from `docs/benchmarks.md` referenced. | Doc exists; criteria are measurable (not vague); frontmatter CI gate passes. | Done |

## Phase 5: Performance scaling

**Goals**: GPU acceleration from postprocessing to solver kernel.

### Phase 5 checklist

| ID | Priority | Dependencies | Description | Deliverables | Acceptance criteria | Status |
|:---|:---------|:-------------|:------------|:-------------|:--------------------|:-------|
| PH5-CHK-001 | A | — | Lock GPU architecture: choose API (wgpu), target matrix, first-offload candidate, real-hardware validation minimum. Satisfies GAP-007. | New: `docs/gpu-arch.md`; milestone gate sequence G1–G7 defined. | Doc exists; API choice made with rationale; first-offload candidate named; hardware validation minimum specified; frontmatter CI gate passes. | Done |
| PH5-CHK-002 | A | PH5-CHK-001 | Add `wgpu` to `nec_accel`; implement device enumeration and a no-op compute pipeline that compiles and runs in CI via software rasterizer. | Modified: `crates/nec_accel/Cargo.toml`, `src/lib.rs`. New: `src/wgpu_device.rs`. | `cargo test -p nec_accel` passes in CI; device enumeration returns at least one adapter (software fallback). | Done |
| PH5-CHK-003 | A | PH5-CHK-002 | Implement the RP far-field gain WGSL compute shader (gate G3). Numerical parity test on `dipole-freesp-rp-51seg`: GPU RP results within RP tolerance vs CPU reference. | New: WGSL shader file; `HallenFrGpuKernel::execute_wgpu`; parity test. | Parity test passes in CI (software rasterizer); manual validation on real hardware (workstation + Pi5) per §6 of `gpu-arch.md`. | Done |
| PH5-CHK-004 | B | PH5-CHK-003 | Wire `--exec gpu` through the wgpu RP kernel in the CLI (gate G4). Integration test: RP output within tolerance when wgpu adapter available. | Modified: `apps/nec-cli/src/main.rs`, `crates/nec_accel/src/lib.rs`. New: integration test. | CLI RP output matches CPU within tolerance; `cargo test -p nec-cli` clean. | Not started |
| PH5-CHK-005 | B | PH5-CHK-004 | CPU-vs-GPU benchmark gate (gate G5): benchmark RP path on large RP grid; assert GPU ≥ 0.8× CPU (no more than 20% slower than CPU as regression guard). | Modified: `scripts/pi-benchmark-compare.sh` or new script; benchmark results in `docs/benchmarks.md`. | Benchmark gate added to CI; timing comparison documented. | Not started |
| PH5-CHK-006 | B | PH5-CHK-005 | Prototype Hallen Z-matrix fill WGSL kernel (gate G6). Numerical parity: filled Z-matrix elements match CPU within 1×10⁻⁶ relative on `dipole-freesp-51seg`. | New: WGSL shader for Z-matrix fill; parity test. | Parity test passes in CI. | Not started |
| PH5-CHK-007 | C | PH5-CHK-006 | Full GPU Hallen solve path (gate G7): GPU matrix-fill + solve; all corpus impedance tolerance gates pass. | Modified: `nec_accel`, `nec_solver`. | All corpus decks pass impedance tolerance gates with GPU path; `cargo test` clean. | Not started |

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
| GAP-002 | NEC-4 feature boundary | **CRITICAL** | Phase 2 end | Parser+Solver | BLK-002: explicit list of supported/deferred NEC-4 cards/features is current in `docs/nec4-support.md` and referenced from roadmap + CLI docs. |
| GAP-003 | MVP ground model set | **HIGH** | Phase 2 end | Solver | `dipole-ground-51seg`, `dipole-gn0-fresnel-51seg`, `dipole-gn2-deferred`, and `dipole-gn2-near-ground-51seg` pass CI tolerance/contract gates, and `docs/nec4-support.md` documents the in-scope GN subset plus deferred Sommerfeld/buried rationale. |
| GAP-004 | Plugin/scripting interface | **HIGH** | Phase 3 end | Core APIs+Automation | BLK-004: extension API design, safety model, and first two working extension points are documented and exercised by at least one integration example. |
| GAP-005 | 4nec2-like text report format | **HIGH** | Phase 1 end | CLI+Reporting | **Resolved 2026-04-23** via PAR-001 v1 contract and CI gate. |
| GAP-006 | GUI information architecture | **MEDIUM** | Phase 3 end | GUI+UX | IA document, task flow wireframes, and at least one round of workflow-feedback notes exist for sweep setup, result inspection, and rerun workflows. |
| GAP-007 | GPU rollout criteria | **MEDIUM** | Phase 5 end | Acceleration | **Resolved 2026-05-03** via `docs/gpu-arch.md` (PH5-CHK-001): wgpu chosen as primary API; target matrix defined (Vulkan primary, Metal/DX12/OpenCL via wgpu); first-offload candidate = RP far-field gain; real-hardware validation minimum = G3 passes on workstation + Pi5 before matrix-fill work begins; milestone gate sequence G1–G7 defined. |
| GAP-008 | Dependency/license policy | **MEDIUM** | Phase 2 end | Build+Release | BLK-005: policy thresholds, exception process, and GPLv2 compatibility rules are documented and referenced by release/process docs. |
| GAP-009 | Workflow parity acceptance criteria | **HIGH** | Phase 3 start | Product+GUI/CLI | At least one measurable usability benchmark exists per Phase 3 milestone, with explicit step-count or elapsed-time acceptance criteria against a named incumbent workflow. |
| GAP-010 | Automation and embedding strategy | **HIGH** | Phase 3 start | Core APIs | A documented automation surface exists for non-GUI consumers, including error model, stability expectations, and planned bindings/embedding path. |
| GAP-011 | Classic batch-CLI parity definition | **MEDIUM** | Phase 2 end | CLI | Batch-CLI contract explicitly covers exit codes, stdout/stderr behavior, machine-parseable report structure, and non-interactive sweep/bench workflows relative to open NEC2 tools. |
| GAP-012 | AutoEZ-class automation acceptance | **HIGH** | Phase 3 end | Automation | Acceptance checklist covers variable sweeps, resonance targeting, convergence studies, and matching-network helpers with at least one documented end-to-end workflow per class. |
| GAP-013 | NEC-5-informed validation matrix | **HIGH** | Phase 2 end | Validation | Validation-manual-informed case matrix is maintained with owner, corpus mapping, and tolerance-gated status for each in-scope class. |
| GAP-014 | External optimizer-loop compatibility | **MEDIUM** | Phase 3 end | Automation+CLI | Deterministic objective-evaluation CLI/API contract is documented and at least one xnec2c-optimize-style loop is reproduced end-to-end with stable machine-readable outputs. |

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
| BLK-004 | Plugin API design, safety model, first two extension points working | **Resolved 2026-04-30**: `docs/plugin-api-design.md` documents EP-1 (`DeckPostProcessor`) and EP-2 (`ResultFilter`); both exercised by doctests. |
| BLK-005 | GPLv2 dependency policy thresholds and exception process documented | **Resolved 2026-05-02**: `docs/dependency-policy.md` authored with SPDX allowlist, deny-list, GPLv2 compatibility rules, and exception process; `deny.toml` configured; `cargo deny check licenses` clean. |

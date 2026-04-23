---
project: fnec-rust
doc: docs/requirements.md
status: living
last_updated: 2026-04-23
---

# Requirements

## Scope decisions (confirmed)

- **DEC-001**: Support both NEC-2 and NEC-4 compatibility goals, delivered incrementally.
- **DEC-002**: Ground model scope starts simple for fast progress; more complex ground patterns are added later.
- **DEC-003**: GPU acceleration starts in postprocessing only; matrix fill and solve acceleration are deferred.
- **DEC-004**: User-facing output is 4nec2-like text output only for now; JSON and CSV are out of scope in early phases.
- **DEC-005**: GUI direction is modern, intuitive, and task-oriented (not a legacy 4nec2 dialog clone).
- **DEC-006**: Plugin and scripting capabilities are in scope.
- **DEC-007**: License compatibility risk is tracked and evaluated continuously via SBOM and dependency review.
- **DEC-008**: GPU acceleration prioritizes FOSS-based frameworks (e.g., OpenCL, SYCL, HIP) over proprietary stacks. Within FOSS frameworks, AMD GPUs are preferred over Intel and NVIDIA for vendor diversity and ecosystem growth.
- **DEC-009**: Product parity targets are explicit: fnec-rust aims to be at least equal to NEC-2/NEC-4 in supported-scope accuracy, equal to 4nec2 and EZNEC in mainstream workflow coverage, competitive with AutoEZ in automation-driven design workflows, competitive with xnec2c-optimize for optimizer-loop orchestration, and competitive with xnec2c, yeti01/nec2, and necpp in open-source workflow, batch execution, and embeddability.

## Functional requirements

- **FR-001**: The core solver must be implemented in Rust as reusable crates.
- **FR-002**: The project must provide CLI and GUI frontends; TUI is optional.
- **FR-003**: The solver must parse and execute NEC deck files used in real-world 4nec2 workflows.
- **FR-004**: The project must support Markdown-based project import and export in addition to NEC decks.
- **FR-005**: The system must provide 4nec2-like text reports for core analysis results.
- **FR-006**: A plugin/scripting extension mechanism must be designed and implemented in phases.
- **FR-007**: The system must support deterministic batch and sweep workflows suitable for optimizer-driven and scripted studies.
- **FR-008**: The system must expose stable automation-oriented core APIs so non-GUI consumers can embed solver workflows without shelling out to brittle text parsing.
- **FR-009**: The system must provide geometry diagnostics that catch invalid, ambiguous, or numerically fragile models early with actionable messages.
- **FR-010**: The system must eventually support automation helpers for resonance targeting, convergence studies, and matching-network-oriented workflows comparable in practical value to AutoEZ.

## Non-functional requirements

- **NFR-001**: Primary runtime target is Linux (Wayland), then macOS, then Windows.
- **NFR-002**: CPU execution must be multithreaded and deterministic by default.
- **NFR-003**: GPU acceleration must be optional at runtime with reliable CPU fallback.
- **NFR-004**: Numerical compatibility must be measured against a reference with explicit tolerances per metric.
- **NFR-005**: CLI execution must remain stable and script-friendly, with predictable stdin/stdout/stderr behavior suitable for UNIX batch workflows.
- **NFR-006**: For supported workflows, usability must be competitive with incumbent tools, not just numerically correct; result inspection and repeat-run iteration must be measurably efficient.

## Compatibility requirements

- **COMP-001**: NEC parsing must be tolerant enough for real-world 4nec2 decks. 4nec2 is the primary compatibility standard.
- **COMP-002**: Numerical output must be comparable to reference outputs under a defined tolerance matrix.
- **COMP-003**: NEC-4 scope must be versioned and explicit so partial support is visible to users.
- **COMP-004**: The parser must support an xnec2c input dialect as a secondary mode without affecting 4nec2 primary behaviour.
- **COMP-005**: Dialect auto-detection must identify the input type before parsing begins; ambiguous input defaults to 4nec2 with a diagnostic.
- **COMP-006**: Explicit dialect override must be available as a CLI option and as a project frontmatter field.
- **COMP-007**: Dialect-specific logic must be architecturally isolated so the 4nec2 parser path has no dependency on xnec2c dialect code.
- **COMP-008**: For supported model classes, numerical accuracy must be at least NEC-2/NEC-4-class and must never regress below the documented tolerance matrix without an explicit contract change.
- **COMP-009**: Feature planning must explicitly track parity against 4nec2 and EZNEC for mainstream amateur and professional antenna-design workflows, including sweeps, report content, gain/pattern inspection, and iterative design tasks.
- **COMP-010**: Open-source competitiveness must explicitly track xnec2c, yeti01/nec2, and necpp so fnec-rust does not fall behind on Linux workflows, classic batch execution, or embeddable automation use cases.
- **COMP-011**: CLI behavior must remain sufficient to replace classic open NEC batch tools for routine automated runs.
- **COMP-012**: Library and automation surfaces must be designed so fnec-rust can compete with necpp-style embedding in optimization, research, and service contexts.
- **COMP-013**: Automation workflow planning must explicitly track AutoEZ-class capabilities such as variable-driven studies, resonance search, convergence studies, and matching-network assistance.
- **COMP-014**: Validation planning must include an explicit case matrix informed by the NEC-5 Validation Manual categories (kernel behavior, source modeling, convergence, junction/surface classes, and loops/wires over ground), with mapped tolerance-gated corpus coverage for in-scope equivalents.
- **COMP-015**: Automation CLI/API contracts must be stable enough to support external optimizer loops comparable to xnec2c-optimize-style objective-driven runs.

## Documentation and process requirements

- **DOC-001**: Every Markdown file under docs must include YAML frontmatter.
- **DOC-002**: Frontmatter must include project, doc, status, and last_updated.
- **DOC-003**: Pull request automation must validate doc frontmatter and stamp last_updated for changed docs.
- **DOC-004**: Documentation must track roadmap, architecture, design, backlog, changelog, release notes, steering, memories, and SBOM.
- **DOC-005**: A version bump PR must update docs/changelog.md and docs/releasenotes.md or CI must fail.

## Numerical compatibility tolerance matrix

The goal is near-100% numerical parity with the reference engine.  
All comparisons are against a curated reference corpus run through the authoritative NEC-2/NEC-4 reference implementation.  
Tolerances below define the strictest acceptable deviation; any failure is a defect.

### Metric targets

| Metric | Symbol | Target tolerance | Rationale |
|:-------|:------:|:----------------:|:----------|
| Input resistance | R (Ω) | ≤ 0.1 % relative, or ≤ 0.05 Ω absolute (whichever is wider) | Near-exact parity; small absolute floor allows for trivially close cases |
| Input reactance | X (Ω) | ≤ 0.1 % relative, or ≤ 0.05 Ω absolute (whichever is wider) | Same as resistance |
| Maximum gain | dBi | ≤ 0.05 dB | Near-exact parity with reference |
| Pattern gain samples | dBi per sample | ≤ 0.1 dB | Slightly looser to allow for angular interpolation alignment |
| Segment current magnitude | A | ≤ 0.1 % relative | Near-exact parity |
| Segment current phase | degrees | ≤ 0.1 ° | Near-exact parity |
| SWR (derived) | — | ≤ 0.01 absolute | Derived metric; tight because R/X targets already bind it |

### Tolerance policy

- If a reference value is near zero the absolute floor applies to avoid meaninglessly large relative errors.
- Tolerances are versioned; any change is a breaking API change on the compatibility contract and requires a COMP-002 note in the changelog.
- Exceeding any target for a corpus case is a CI failure, not a warning.
- The corpus must include at minimum: half-wave dipole (free space), half-wave dipole (over ground), Yagi, loaded element, frequency sweep, multi-source case.

### Tolerance as a validation philosophy

The tolerance matrix is not just acceptance criteria; it is a design contract that shapes the solver architecture and testing discipline.

**Core principles:**

1. **Reference-centric**: All numerical work is measured against a curated golden corpus run through the authoritative NEC-2/NEC-4 reference. No internal-only validation is acceptable.

2. **Metric specificity**: Different metrics have different precision targets. Impedance must be near-exact (0.1% R/X); pattern gain can tolerate slightly more deviation (0.1 dB per sample) because angular interpolation varies between implementations. This specificity forces choices in solver precision, quadrature order, and numerical stability.

3. **Absolute and relative floors**: Mixing relative and absolute tolerances avoids paradoxes near zero. A reactance near zero is allowed ±0.05 Ω absolute rather than suffering from meaninglessly wide relative bands. This grounds the contract in physical reality.

4. **CI enforcement**: Exceeding any tolerance on any corpus case is a **CI failure**, not a warning or a deferred issue. This discipline prevents tolerance creep and ensures every merge maintains the compatibility contract.

5. **Versioning the contract**: If a solver improvement or bug fix requires tolerance adjustment, that adjustment is documented in the changelog as a breaking API change. Users are never surprised by what "parity" means.

6. **Staged corpus growth**: The MVP corpus is minimal (dipole, Yagi, loaded element). As phases progress, the corpus grows to include ground effects, frequency sweeps, multi-source cases, and edge cases. Each phase gates on corpus pass at the current scope.

This discipline ensures that fnec-rust's numerical parity is measurable, auditable, and trustworthy for production antenna work.

### GAP-001 resolution

- Status: **Resolved**
- Decision date: 2026-04-22
- Target parity level: near-100% (see table above).

## Gap list (open definition work)

- **GAP-001**: ~~Define numerical tolerance targets~~ — **resolved**, see tolerance matrix above.
- **GAP-002 (critical)**: Define exactly which NEC-4 cards/features are in initial support and which are deferred. **Resolved** 2026-04-23: see `docs/nec4-support.md` for complete card support matrix and phase assignments.
- **GAP-003 (high)**: Define MVP ground model set and upgrade path for advanced ground behavior.
- **GAP-004 (high)**: Specify the first plugin/scripting interface (command hooks, sandboxing, API stability).
- **GAP-005 (high)**: Define text report format contract for 4nec2-like output (sections, units, precision, ordering).
- **GAP-006 (medium)**: Define GUI information architecture for a modern task-oriented workflow.
- **GAP-007 (medium)**: Define GPU rollout criteria from postprocess to matrix fill and solve. Framework selection must follow DEC-008 (FOSS-first, AMD-preferred).
- **GAP-008 (medium)**: Define dependency/license policy thresholds and exception handling for GPLv2 compatibility.
- **GAP-009 (high)**: Define measurable acceptance criteria for 4nec2/EZNEC-grade workflow parity, including result inspection, sweep interaction, and reporting completeness.
- **GAP-010 (high)**: Define stable automation and embedding strategy for non-Rust consumers so fnec-rust can compete with necpp-style integrations.
- **GAP-011 (medium)**: Define classic batch-CLI parity requirements relative to open NEC2 tools such as yeti01/nec2 and xnec2c batch-oriented workflows.
- **GAP-012 (high)**: Define measurable acceptance criteria for AutoEZ-class automation parity, including variable sweeps, resonance targeting, convergence studies, and matching-network workflows.
- **GAP-013 (high)**: Define and maintain a NEC-5-validation-manual-informed case matrix that maps target classes to corpus tests and tolerance gates.
- **GAP-014 (medium)**: Define measurable external optimizer-loop compatibility criteria relative to xnec2c-optimize workflows (objective input, deterministic run behavior, and machine-readable outputs).

## Acceptance criteria

- [ ] Scope decisions are reflected consistently in architecture, design, and roadmap docs.
- [ ] Gap items have owners, target milestone, and resolution criteria.
- [ ] Compatibility test corpus and tolerance matrix are documented before broad solver expansion.
- [ ] Text report format contract is fixed before GUI result views are finalized.
- [ ] Parity targets against NEC-2, NEC-4, NEC-5, 4nec2, EZNEC, AutoEZ, xnec2c, xnec2c-optimize, yeti01/nec2, and necpp are reflected consistently in roadmap and architecture.
- [ ] Workflow parity requirements are specific enough to test, not just aspirational.
- [ ] Automation and embedding expectations are explicit enough to drive API design decisions.
- [ ] NEC-5-validation-manual-informed case classes are mapped to explicit tolerance-gated corpus coverage for in-scope scenarios.

## Report format contract (PAR-001 v1)

The CLI text report format is versioned and contract-bound for compatibility testing.

- Header line 1: `FNEC FEEDPOINT REPORT`
- Header line 2: `FORMAT_VERSION 1`
- Metadata lines: `FREQ_MHZ`, `SOLVER_MODE`, `PULSE_RHS`
- Section line: `FEEDPOINTS`
- Table header: `TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM`
- Data rows: one source-driven segment per row; exactly 8 whitespace-separated columns
- Numeric formatting: fixed-point with 6 decimals
- Ordering: preserve solver/feed discovery order emitted by CLI for deterministic runs

Contract changes to these tokens, column order, or precision rules require explicit changelog note and test updates.

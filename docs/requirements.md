---
project: fnec-rust
doc: docs/requirements.md
status: living
last_updated: 2026-04-22
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

## Functional requirements

- **FR-001**: The core solver must be implemented in Rust as reusable crates.
- **FR-002**: The project must provide CLI and GUI frontends; TUI is optional.
- **FR-003**: The solver must parse and execute NEC deck files used in real-world 4nec2 workflows.
- **FR-004**: The project must support Markdown-based project import and export in addition to NEC decks.
- **FR-005**: The system must provide 4nec2-like text reports for core analysis results.
- **FR-006**: A plugin/scripting extension mechanism must be designed and implemented in phases.

## Non-functional requirements

- **NFR-001**: Primary runtime target is Linux (Wayland), then macOS, then Windows.
- **NFR-002**: CPU execution must be multithreaded and deterministic by default.
- **NFR-003**: GPU acceleration must be optional at runtime with reliable CPU fallback.
- **NFR-004**: Numerical compatibility must be measured against a reference with explicit tolerances per metric.

## Compatibility requirements

- **COMP-001**: NEC parsing must be tolerant enough for real-world 4nec2 decks. 4nec2 is the primary compatibility standard.
- **COMP-002**: Numerical output must be comparable to reference outputs under a defined tolerance matrix.
- **COMP-003**: NEC-4 scope must be versioned and explicit so partial support is visible to users.
- **COMP-004**: The parser must support an xnec2c input dialect as a secondary mode without affecting 4nec2 primary behaviour.
- **COMP-005**: Dialect auto-detection must identify the input type before parsing begins; ambiguous input defaults to 4nec2 with a diagnostic.
- **COMP-006**: Explicit dialect override must be available as a CLI option and as a project frontmatter field.
- **COMP-007**: Dialect-specific logic must be architecturally isolated so the 4nec2 parser path has no dependency on xnec2c dialect code.

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

### GAP-001 resolution

- Status: **Resolved**
- Decision date: 2026-04-22
- Target parity level: near-100% (see table above).

## Gap list (open definition work)

- **GAP-001**: ~~Define numerical tolerance targets~~ — **resolved**, see tolerance matrix above.
- **GAP-002 (critical)**: Define exactly which NEC-4 cards/features are in initial support and which are deferred.
- **GAP-003 (high)**: Define MVP ground model set and upgrade path for advanced ground behavior.
- **GAP-004 (high)**: Specify the first plugin/scripting interface (command hooks, sandboxing, API stability).
- **GAP-005 (high)**: Define text report format contract for 4nec2-like output (sections, units, precision, ordering).
- **GAP-006 (medium)**: Define GUI information architecture for a modern task-oriented workflow.
- **GAP-007 (medium)**: Define GPU rollout criteria from postprocess to matrix fill and solve.
- **GAP-008 (medium)**: Define dependency/license policy thresholds and exception handling for GPLv2 compatibility.

## Acceptance criteria

- [ ] Scope decisions are reflected consistently in architecture, design, and roadmap docs.
- [ ] Gap items have owners, target milestone, and resolution criteria.
- [ ] Compatibility test corpus and tolerance matrix are documented before broad solver expansion.
- [ ] Text report format contract is fixed before GUI result views are finalized.

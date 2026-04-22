---
project: fnec-rust
doc: docs/roadmap.md
status: living
last_updated: 2026-04-22
---

# Roadmap

## Phase 0 (done/in place)

- Documentation baseline established under docs.
- PR-based last_updated automation path defined for protected main.

## Phase 1 (current focus): NEC foundation and fast progress

- Implement solver core as reusable Rust crates.
- Start with simple ground handling only.
- Implement 4nec2-like text output contract.
- Build CLI-first execution flow.
- Draft tolerance matrix for numerical comparison.

## Phase 2: Compatibility expansion and confidence

- Expand NEC-2 support breadth.
- Introduce clearly scoped NEC-4 support subset.
- Build golden test corpus and reference comparison workflows.
- Lock acceptance tolerances for impedance/gain/pattern checks.

## Phase 3: UX and workflow productization

- Build modern, intuitive, task-oriented GUI on iced.
- Add project-oriented workflows for import/export and run orchestration.
- Keep CLI and GUI behavior aligned for core tasks.

## Phase 4: Extensibility

- Introduce plugin/scripting architecture.
- Deliver first stable extension points and documentation.

## Phase 5: Performance scaling

- Add GPU acceleration in postprocessing.
- Benchmark and stabilize CPU/GPU selection behavior.
- Plan staged expansion toward matrix fill and solve offload.

## Gap-driven milestone blockers

- **BLK-001**: Tolerance matrix definition (GAP-001 in requirements).
- **BLK-002**: NEC-4 feature boundary for initial release (GAP-002).
- **BLK-003**: 4nec2-like report format contract (GAP-005).
- **BLK-004**: Plugin API and safety model baseline (GAP-004).
- **BLK-005**: GPLv2 dependency policy enforcement threshold (GAP-008).

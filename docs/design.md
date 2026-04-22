---
project: fnec-rust
doc: docs/design.md
status: living
last_updated: 2026-04-22
---

# Design

## Product design direction

- UX is modern, intuitive, and task-oriented.
- Workflows prioritize: open/import project, configure run, execute, inspect results, iterate.
- UI design should avoid reproducing legacy complexity where clearer flows are possible.

## Interaction model

- CLI is the canonical execution path and baseline for correctness.
- GUI organizes user tasks as guided workflows rather than low-level card editing dialogs.
- Optional TUI supports operational and headless workflows.

## Output design

- Primary result presentation is 4nec2-like text output.
- JSON/CSV exports are intentionally deferred.
- Text output sections, units, and precision require a fixed format contract.

## Incremental design strategy

- Begin with simple ground model controls.
- Add advanced ground configuration progressively.
- Begin with CPU-first workflows and add GPU postprocessing controls later.

## Extensibility design

- Plugin/scripting is in scope.
- Initial extension model should focus on safe, bounded hooks.
- Extension lifecycle and compatibility policy must be documented before public plugin API freeze.

## Documentation design constraints

- Docs files must keep standard frontmatter and PR-based update flow.
- last_updated remains CI-managed.

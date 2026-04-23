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
- UX quality is measured against real incumbents: 4nec2 and EZNEC for mainstream antenna-design workflows, AutoEZ and xnec2c-optimize for advanced automation-driven study workflows, xnec2c for open-source Linux workbench usability, and classic NEC batch tools for automation efficiency.

## Interaction model

- CLI is the canonical execution path and baseline for correctness.
- GUI organizes user tasks as guided workflows rather than low-level card editing dialogs.
- Optional TUI supports operational and headless workflows.
- CLI must remain strong for scripted and optimizer-driven operation; GUI improvements must not come at the cost of batch-friendliness.
- GUI should eventually exceed legacy workflows for common tasks such as sweep setup, result inspection, and iterative tuning, even if it does not mimic their layout.
- Automation design should reduce dependence on external spreadsheet-style orchestration by bringing high-value study workflows into the product over time while still supporting robust external optimizer-loop integration.

## Output design

- Primary result presentation is 4nec2-like text output.
- JSON/CSV exports are intentionally deferred.
- Text output sections, units, and precision require a fixed format contract.
- Text output should be good enough to act as a daily comparison/reporting surface for users familiar with 4nec2, while remaining predictable enough for machine parsing in automation contexts.

## Incremental design strategy

- Begin with simple ground model controls.
- Add advanced ground configuration progressively.
- Begin with CPU-first workflows and add GPU postprocessing controls later.
- Sequence work so accuracy parity, reporting parity, and workflow parity advance together; a numerically strong but operationally weak product does not meet the project goal.
- Sequence work so accuracy parity, reporting parity, and workflow parity advance together; a numerically strong but operationally weak product does not meet the project goal.
- Prioritize validation UX that makes NEC-5-manual-inspired scenario classes (convergence, source behavior, loops/ground, and junction stress cases) easy to run and compare repeatedly.

## Extensibility design

- Plugin/scripting is in scope.
- Initial extension model should focus on safe, bounded hooks.
- Extension lifecycle and compatibility policy must be documented before public plugin API freeze.
- Automation design should support future bindings and embedding scenarios comparable to necpp-style library use, not only in-process plugins.
- Automation design should also support higher-level user workflows comparable in practical value to AutoEZ's variable studies, resonance tools, and repeated-analysis helpers.

## Documentation design constraints

- Docs files must keep standard frontmatter and PR-based update flow.
- last_updated remains CI-managed.

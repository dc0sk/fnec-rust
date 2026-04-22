---
project: fnec-rust
doc: docs/architecture.md
status: living
last_updated: 2026-04-22
---

# Architecture

## System goals

- Build a Rust-native NEC-compatible solver with incremental NEC-2 and NEC-4 support.
- Provide a reusable core with separate CLI, GUI, and optional TUI frontends.
- Prioritize fast progress with simple ground handling first.
- Preserve room for extension through plugin/scripting capabilities.

## Core architecture

1. Parse NEC deck input into an AST and diagnostics model.
2. Lower into validated domain model.
3. Build segmentation and physics model inputs.
4. Assemble and solve the numerical system on CPU.
5. Postprocess results (impedance, currents, radiation patterns).
6. Render user-facing 4nec2-like text reports.

## Frontend architecture

- CLI is the first production frontend and reference behavior.
- GUI on iced follows a modern, intuitive, task-oriented workflow.
- Optional TUI on ratatui shares core use cases.
- Frontends consume stable core APIs and must not embed solver logic.

## Performance architecture

- CPU multithreading is baseline.
- Runtime acceleration mode selects CPU or GPU path when available.
- Initial GPU scope is postprocessing only; further offload is staged.

## Extensibility architecture

- Plugin/scripting layer is in scope and planned as explicit extension points.
- Extension API must not break solver determinism guarantees by default.

## Compatibility architecture constraints

- **4nec2 is the primary compatibility target** for both execution and workflow expectations.
- xnec2c input dialect is a secondary compatibility mode; it must not dilute or alter the 4nec2 primary standard.
- Text output format requires a stable contract before broad UI expansion.
- Numerical parity requires a tolerance matrix and reference corpus.

## Input dialect model

fnec-rust supports multiple NEC input dialects through a layered parser architecture:

| Dialect | Status | Notes |
|:--------|:------:|:------|
| 4nec2 | Primary | The canonical target; all real-world 4nec2 decks must parse correctly |
| xnec2c | Secondary | Where xnec2c input diverges from 4nec2, it is treated as a distinct dialect |

### Auto-detection

- The parser attempts automatic dialect detection before user-visible mode selection.
- Detection is heuristic and based on structural markers unique to each dialect.
- If detection is ambiguous, the parser defaults to 4nec2 mode and emits an informational diagnostic.
- Users may override dialect detection explicitly via a CLI flag or project frontmatter field.
- A detected dialect is recorded in the project model and surfaced in report headers.

### Implementation rule

- Dialect-specific parsing logic must be isolated behind a dialect trait or enum; it must not be mixed into the shared NEC2 core parser.
- The 4nec2 dialect path must remain independently testable with no xnec2c-specific code on its execution path.

## Reference engine: xnec2c

- xnec2c (KJ7LNW fork, https://github.com/KJ7LNW/xnec2c) is used as the authoritative NEC2 reference engine for:
  - Generating golden test corpus outputs against which fnec-rust numerical results are compared.
  - Algorithmic study of NEC2 numerical methods as implemented in C.
- **License constraint**: xnec2c is GPL-3.0-only. fnec-rust is GPL-2.0-only. No code from xnec2c may be copied, translated, or adapted into fnec-rust source under any circumstances.
- xnec2c's `examples/` and `t/` directories are the primary source for the test corpus `.nec` input files.

## Additional reference implementations

The following projects are useful comparative references for algorithms, behavior checks, and implementation ideas:

- M5AIQ NEC notes/tools: https://www.qsl.net/m5aiq/nec.html
- yeti01/nec2: https://github.com/yeti01/nec2
- tmolteno/necpp: https://github.com/tmolteno/necpp

These references are supplementary. 4nec2 compatibility remains the primary product target, and xnec2c remains the main NEC2 parity reference corpus source.

## Documentation process constraints

- Docs updates flow through PRs only due to protected main.
- Frontmatter validation and stamping automation remain required quality gates.

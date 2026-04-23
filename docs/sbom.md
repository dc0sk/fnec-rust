---
project: fnec-rust
doc: docs/sbom.md
status: living
last_updated: 2026-04-22
---

# SBOM

This SBOM tracks all significant components used in or referenced by fnec-rust, including license, authors, activity status, and source URL.  
Update this file whenever a dependency is added, removed, or its status changes.

## Conventions

- Activity: Active | Maintenance | Inactive | Unknown
- Role: Runtime | Dev | CI | Reference (reference = used for validation/study only, not linked or compiled into fnec-rust)
- License column uses SPDX identifiers.

## Project crates (fnec-rust internal)

| Component | Role | License | Activity | Source |
|:----------|:----:|:-------:|:--------:|:------|
| nec_parser | Runtime | GPL-3.0-only | Active | crates/nec_parser |
| nec_model | Runtime | GPL-3.0-only | Active | crates/nec_model |
| nec_solver | Runtime | GPL-3.0-only | Active | crates/nec_solver |
| nec_accel | Runtime | GPL-3.0-only | Active | crates/nec_accel |
| nec_report | Runtime | GPL-3.0-only | Active | crates/nec_report |
| nec_project | Runtime | GPL-3.0-only | Active | crates/nec_project |
| nec-cli | Runtime | GPL-3.0-only | Active | apps/nec-cli |
| nec-gui | Runtime | GPL-3.0-only | Active | apps/nec-gui |
| nec-tui | Runtime | GPL-3.0-only | Active | apps/nec-tui |

## External runtime dependencies (to be confirmed as selected)

| Dependency | Purpose | License | Activity | Source |
|:-----------|:--------|:-------:|:--------:|:------|
| rayon | CPU parallelism | MIT OR Apache-2.0 | Active | https://github.com/rayon-rs/rayon |
| iced | GUI framework | MIT | Active | https://github.com/iced-rs/iced |
| ratatui | TUI framework | MIT | Active | https://github.com/ratatui-org/ratatui |
| faer | Linear algebra | MIT OR Apache-2.0 | Active | https://github.com/sarah-ek/faer-rs |
| wgpu | GPU compute backend | MIT OR Apache-2.0 | Active | https://github.com/gfx-rs/wgpu |

**License note**: all external runtime dependencies must be compatible with GPL-3.0-only distribution.
Verify each crate's SPDX before adding it. MIT, Apache-2.0, BSD, GPL-3.0-only, and LGPL-2.1-or-later variants are compatible. GPL-2.0-only is **not** compatible.

## CI and tooling

| Tool | Purpose | License | Activity | Source |
|:-----|:--------|:-------:|:--------:|:------|
| GitHub Actions | CI/CD | Proprietary (SaaS) | Active | https://github.com/features/actions |
| actions/checkout | Checkout action | MIT | Active | https://github.com/actions/checkout |
| cargo | Rust build/test | MIT OR Apache-2.0 | Active | https://github.com/rust-lang/cargo |
| rustfmt | Code formatting | MIT OR Apache-2.0 | Active | https://github.com/rust-lang/rustfmt |
| clippy | Linting | MIT OR Apache-2.0 | Active | https://github.com/rust-lang/rust-clippy |
| cargo-sbom | SPDX SBOM generation (run on version bump) | MIT OR Apache-2.0 | Active | https://github.com/psastras/sbom-rs |

## Reference implementations (study and validation only — no code inclusion)

| Component | Role | License | Activity | Source | Notes |
|:----------|:----:|:-------:|:--------:|:------|:------|
| xnec2c (KJ7LNW fork) | Reference | GPL-3.0-only | Active | https://github.com/KJ7LNW/xnec2c | **No code may be copied or adapted into fnec-rust.** Used as reference engine to generate golden test corpus outputs and as algorithmic study material only. |
| yeti01/nec2 | Reference | Unknown | Maintenance | https://github.com/yeti01/nec2 | Open NEC2 FORTRAN baseline for classic batch execution and Sommerfeld/Norton-capable CLI behavior. Study/validation only. |
| tmolteno/necpp | Reference | GPL-2.0-only | Active | https://github.com/tmolteno/necpp | Embeddable NEC2-compatible library baseline for automation, bindings, and geometry diagnostics. Study only; **must not be incorporated** due to GPL-2.0-only incompatibility. |
| 4nec2 | Reference | Proprietary freeware | Maintenance | https://www.4nec2.net/ | Primary product workflow comparator and fallback external reference when xnec2c is unsuitable. No code inclusion. |
| EZNEC Pro+ | Reference | Proprietary freeware | Maintenance | https://www.eznec.com/ | Workflow and usability comparator, especially for mainstream antenna-design tasks. No code inclusion. |
| AutoEZ | Reference | Proprietary commercial | Active | https://www.eznec.com/AutoEZ.htm | Automation-workflow comparator for variable studies, resonance search, convergence analysis, matching-network helpers, and orchestration. Purchase deferred until Phase 3 workflow benchmarking. |

## License compatibility policy (GAP-008)

- fnec-rust is GPL-3.0-only.
- All runtime and dev dependencies must be GPL-3.0-compatible (MIT, Apache-2.0, BSD, GPL-3.0-only, LGPL-2.1-or-later are acceptable).
- GPL-2.0-only code is **incompatible** and must never be incorporated.
- Reference-role entries are exempt from the compatibility requirement but must be clearly marked.

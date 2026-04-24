# fnec-rust

![version](https://img.shields.io/badge/version-0.1.0-blue)
![license](https://img.shields.io/badge/license-GPL--3.0--only-blue)

fnec-rust is a Rust-native antenna modeling workspace targeting near-100% practical compatibility with 4nec2, while keeping the codebase modular, testable, and portable.

## Features

- Parse 4nec2 / NEC2 deck files (GW, GE, GN, EX, FR, EN cards)
- Hallén MoM solver — physically accurate feedpoint impedance for thin-wire antennas
  - Validated: 51-segment λ/2 dipole at 14.2 MHz → **74.24 + j13.90 Ω** (matches Python reference)
	- GN 1 (perfect ground at z=0) is supported via image method; `dipole-ground-51seg` regression is **81.91 + j16.42 Ω**
	- GN types other than 1 are currently deferred; fnec warns and falls back to free-space behavior
	- GE ground-reflection flag is parsed; non-zero values currently emit a warning and are ignored in Phase 1
	- Current scope: collinear wire sets aligned with the driven segment axis; non-collinear Hallén topologies fail fast with an explicit error
- Pulse-basis, continuity-basis, and sinusoidal-tapered Pocklington solvers (EXPERIMENTAL — known to diverge for thin wires)
	- `sinusoidal` now falls back to Hallen on single collinear chains when its residual budget is exceeded, so the CLI avoids returning misleading impedances for that path
- CLI binary `fnec` with selectable solver and RHS modes
- FR sweep support in CLI output and corpus validation gating
- Residual diagnostics printed to stderr on every run
- Modular crate workspace: parser, model, solver, accel, report, project, CLI, GUI, TUI

## Quick start

```
cargo build --release
./target/release/fnec dipole.nec
```

Example deck (`dipole.nec`):

```
GW 1 51 0 0 -5.282 0 0 5.282 0.001
GE
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
EN
```

Example output (report contract v1):

```
FNEC FEEDPOINT REPORT
FORMAT_VERSION 1
FREQ_MHZ 14.200000
SOLVER_MODE hallen
PULSE_RHS Nec2

FEEDPOINTS
TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM
1 26 1.000000 0.000000 0.013013 -0.002436 74.242874 13.899516
```

See [docs/cli-guide.md](docs/cli-guide.md) for full option reference.

## Support fnec-rust

[![Donate via PayPal](https://img.shields.io/badge/Donate-PayPal-blue.svg)](https://www.paypal.com/donate/?hosted_button_id=WY9U4MQ3ZAQWC)

If this project helps your work, please consider supporting ongoing development:

https://www.paypal.com/donate/?hosted_button_id=WY9U4MQ3ZAQWC

## Goals

- Primary compatibility target: 4nec2 input and workflow expectations
- Secondary input dialect support: xnec2c where it provides real benefit
- Solver implemented from scratch in Rust
- CLI, GUI, and optional TUI frontends over a shared core
- Multithreaded CPU execution with staged GPU acceleration
- Explicit Linux-on-ARM targets: Raspberry Pi 4 (VideoCore VI GPU) and Raspberry Pi 5 (VideoCore VII GPU)

## Workspace layout

```text
crates/
	nec_parser/
	nec_model/
	nec_solver/
	nec_accel/
	nec_report/
	nec_project/
apps/
	nec-cli/
	nec-gui/
	nec-tui/
docs/
```

## Contributor setup

Install the required cargo tools:

```bash
cargo install cargo-audit cargo-sbom
```

Install the local git hooks:

```bash
make install-hooks
```

This configures `core.hooksPath` to use `.githooks/`.

## Local workflow checks

- Pre-commit: `cargo fmt --all -- --check`, `cargo test --workspace`
- Pre-push: `cargo audit`
- Docs validation: `./scripts/validate-docs-frontmatter.sh`
- Remote ARM64 check (Pi over SSH): `bash ./scripts/pi-remote-workspace-check.sh <user@host>`

### Troubleshooting: Mobile approval dialogs

If you review or approve workflow/tool actions from a mobile device, some approval dialogs may be truncated, fail to render all options, or not submit reliably.

- Prefer desktop browser approval for multi-step actions (branch push, workflow changes, multi-file edits).
- If a mobile approval appears stuck, refresh and retry from desktop rather than re-running commands repeatedly.
- Keep command payloads and commit messages concise when you know approvals may be handled on mobile.

## Version bump workflow

When bumping the workspace version:

1. Update `Cargo.toml`.
2. Regenerate the SPDX SBOM: `cargo sbom --output-format spdx-json > SBOM.spdx.json`
3. Update `docs/changelog.md`.
4. Update `docs/releasenotes.md`.
5. Update documentation references that include the new version.

## Branch policy

`main` stays protected. Work happens on feature branches only, followed by pull requests.

# fnec-rust

fnec-rust is a Rust-native antenna modeling workspace targeting near-100% practical compatibility with 4nec2, while keeping the codebase modular, testable, and portable.

## Support fnec-rust

If this project helps your work, please consider supporting ongoing development:

https://www.paypal.com/donate/?hosted_button_id=WY9U4MQ3ZAQWC

## Goals

- Primary compatibility target: 4nec2 input and workflow expectations
- Secondary input dialect support: xnec2c where it provides real benefit
- Solver implemented from scratch in Rust
- CLI, GUI, and optional TUI frontends over a shared core
- Multithreaded CPU execution with staged GPU acceleration

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
- Docs validation: `./scripts/validate-doc-frontmatter.sh`

## Version bump workflow

When bumping the workspace version:

1. Update `Cargo.toml`.
2. Regenerate the SPDX SBOM: `cargo sbom --output-format spdx-json > SBOM.spdx.json`
3. Update `docs/changelog.md`.
4. Update `docs/releasenotes.md`.
5. Update documentation references that include the new version.

## Branch policy

`main` stays protected. Work happens on feature branches only, followed by pull requests.

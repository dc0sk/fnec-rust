---
project: fnec-rust
doc: docs/par011-dropin-evidence-memo.md
status: living
last_updated: 2026-04-26
---

# PAR-011 Drop-In Compatibility Evidence Memo

This memo collects concrete evidence for PAR-011 (4nec2 solver-binary drop-in compatibility mode) so future implementation can start from verified facts instead of re-discovery.

## Scope

- Target workflow: replace external 4nec2 kernel binaries with fnec-rust-compatible drop-in behavior.
- Current branch status: filename-steered compatibility scaffold exists; full drop-in parity is deferred to Phase 4-5.

## Known artifact inventory

Observed in external NEC2MP package location:

- `nec2dxs500.exe`
- `nec2dxs1K5.exe`
- `nec2dxs3k0.exe`
- `nec2dxs5k0.exe`
- `nec2dxs8k0.exe`
- `nec2dxs11k.exe`
- `nec2mp-readme.pdf`

Observed file metadata snapshot:

- executable timestamps in package: 2012-05-24
- executable size band: ~3.9 MiB to ~4.6 MiB
- no DLL files observed in the package root

SHA256 fingerprint set (captured 2026-04-26):

- `6f623d77e03fcf051a0d581013ad9b503db166d283e41e4ba42999ed285f090b  nec2dxs1K5.exe`
- `6f85368034d27c9f4958db64e0eeb6699592a45731ea7d9b6379aa2e1c9bf0f0  nec2dxs3k0.exe`
- `9ef3368283d72afd808ee79100d333dd213bfcc70ccfb09702e69c755bdc8785  nec2dxs5k0.exe`
- `26e42363d3fa15ab5df6963e0485b79d3c132111f187531c07aafa804dde75e5  nec2dxs8k0.exe`
- `18b1a69d7c7021bfc150f62d26e4797cf9042830ab703c1a93340776266f4fb3  nec2dxs11k.exe`
- `965fb451c44dfef421d4e901accce176383195741b2220bd15742610f34a9b4d  nec2dxs500.exe`
- `ecf97ddcd0ee8db83a6e271dd304174bce78039a7d507803d1949d8060b81df9  nec2mp-readme.pdf`

## Referenced sources

- NEC2MP readme-cited URL: http://users.otenet.gr/~jmsp
- GNU NEC SourceForge project: https://sourceforge.net/projects/gnu-nec/

## NEC2MP readme extracted evidence

Key points from `nec2mp-readme.pdf` extraction:

- Installation workflow expects replacing binaries in `\\4NEC2\\EXE` after creating backups.
- Operator is instructed to select `Nec2dXS*.exe` in 4nec2 settings.
- Command-line hint: `-?` or `-h` for syntax/help.
- DLL/runtime dependencies are not documented in the readme text.

## Evidence checklist

## Binary-name contract

- [ ] Confirm accepted executable-name variants and case-sensitivity behavior used by host tooling.
- [ ] Map each variant to expected segment or model-size bands.

## Installation and replacement workflow

- [ ] Document required copy/replace steps in a standard 4nec2 Windows installation.
- [ ] Confirm whether side-by-side binaries are selected automatically by host tooling.
- [ ] Confirm rollback path for restoring original binaries.

## Invocation and process contract

- [ ] Capture argv shape from host-to-kernel execution.
- [ ] Capture expected working-directory assumptions.
- [ ] Capture stdin/stdout/stderr usage and parsing assumptions.
- [ ] Capture expected exit-code semantics for success and common failure classes.

## Filesystem side effects

- [ ] Enumerate all temporary files written/read by the external-kernel contract.
- [ ] Record overwrite/cleanup lifecycle expectations.
- [ ] Record behavior when output files already exist.

## Dependency surface

- [ ] Confirm runtime DLL dependencies, if any.
- [ ] Confirm search-path/loader assumptions for portable and installed modes.

## Regression fixtures

- [ ] Archive representative call traces for each binary-name variant.
- [ ] Archive canonical input/output pairs suitable for CI compatibility tests.

## Benchmark protocol

- [ ] Define apples-to-apples benchmark method against single-thread legacy kernels.
- [ ] Include segment-band splits aligned to binary-name variants.
- [ ] Capture machine metadata and run settings for reproducibility.

## Current implementation gap summary

- Filename-steered detection is present (`nec2dxs*`, `4nec2*`) and tested in CLI integration tests.
- Execution-mode steering is currently limited to internal mode defaults and diagnostics.
- Full external-kernel drop-in invocation and file-contract parity is not yet implemented.

## Docs-only phased implementation plan

This plan intentionally scopes to documentation and acceptance criteria only. No harness skeleton work is included in this phase.

### Phase 0: Contract capture and freeze

Deliverables:

- Capture Windows installation/replacement contract as step-by-step procedure.
- Capture binary-name matrix with expected size-band intent.
- Capture process contract template (argv, cwd, stdio, exit codes, timeout semantics).

Acceptance tests:

- `AT-PAR011-0001`: document includes exact executable-name matrix and source evidence links.
- `AT-PAR011-0002`: document includes deterministic install rollback procedure.
- `AT-PAR011-0003`: process-contract template reviewed and checked into docs.

### Phase 1: Compatibility behavior specification

Deliverables:

- Define expected behavior for filename-steering defaults and explicit `--exec` override precedence.
- Define warning/diagnostic contract for drop-in profile activation.
- Define expected file side effects and lifecycle constraints.

Acceptance tests:

- `AT-PAR011-0101`: docs specify precedence rule: explicit `--exec` always wins.
- `AT-PAR011-0102`: docs specify warning text classes for steered vs preserved-explicit paths.
- `AT-PAR011-0103`: docs enumerate side-effect files and cleanup expectations.

### Phase 2: Validation and benchmark specification

Deliverables:

- Define compatibility fixture format for external call traces.
- Define benchmark protocol for comparing replacement kernel throughput vs legacy single-thread baseline.
- Define required metadata capture for reproducible benchmark records.

Acceptance tests:

- `AT-PAR011-0201`: fixture schema documented with at least one concrete example record.
- `AT-PAR011-0202`: benchmark protocol includes segment-band and machine metadata requirements.
- `AT-PAR011-0203`: success criteria for parity and performance are explicit and measurable.

### Postponed scope

- Harness skeleton for executing compatibility fixtures is explicitly postponed (user-requested defer of option 3).

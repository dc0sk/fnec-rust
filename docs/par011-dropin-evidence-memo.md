---
project: fnec-rust
doc: docs/par011-dropin-evidence-memo.md
status: draft
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

## Referenced sources

- NEC2MP readme-cited URL: http://users.otenet.gr/~jmsp
- GNU NEC SourceForge project: https://sourceforge.net/projects/gnu-nec/

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

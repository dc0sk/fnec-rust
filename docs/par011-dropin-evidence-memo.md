---
project: fnec-rust
doc: docs/par011-dropin-evidence-memo.md
status: living
last_updated: 2026-10-16
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

## Status of the open items

**2026-10-13:** every item still unticked below is an observation of a real 4nec2
installation on Windows: how it invokes its kernel, what files it writes, which
DLLs it loads. No such installation was available to the project, so none could
be measured, and none was ticked on inference.

**2026-10-15:** a real 4nec2 now runs on the project host under Wine, so most of
these items have been measured; see *Observed under Wine* below. An item ticked
from that capture says "(Wine)". What Windows itself would do differently, such
as loader search paths or long-path handling, is still unobserved, and those
items stay open.

## Observed under Wine (2026-10-15)

**Setup.** 4nec2 V5.9.3 ran under Wine 10.0 in a win64 prefix on Linux, on an Xvfb
display driven with xdotool. Process creation was traced with `WINEDEBUG=+process`.

**Kernel.** The kernel is the one 4nec2 bundles, `nec2dxs1K5.exe`, sha256
`70cf2058…5841`: "Merged nec2d/som2d … Arie Voors", build 2.7. It is a 32-bit
console PE that imports only `KERNEL32.dll` and `msvcrt.dll`.

The fixture pairs are in `apps/nec-cli/tests/fixtures/par011/`, with their
provenance in that directory's README.

### Process contract

**Invocation.** On *Generate*, 4nec2 writes `exe\nec2d.tmp` and runs
`"C:\4nec2\exe\4nec2.bat" gen Nec2dXS1k5 NOW`. The batch file `cd`s into `exe\`
and runs `Nec2dXS1k5.exe < nec2d.tmp`.

| Field | Observed |
|:--|:--|
| argv | the kernel's own name only, with no arguments (`Nec2dXS1k5.exe `) |
| cwd | the 4nec2 `exe` directory |
| stdin | two CRLF lines, the input and output paths, relative to cwd: `..\out\NAME.inp`, `..\out\NAME.out` |
| stdout | prompts (`ENTER NAME OF INPUT FILE >`), which 4nec2 does not read |
| exit code | 0 even when the deck is in error; 4nec2 does not use it |
| success marker | the output file's last line, ` RUN TIME =     0.020` |
| failure | the kernel writes its error text into the `.out` and no `RUN TIME` line; 4nec2 then shows "NEC error ? Check output (F8)", or the error text itself |

**Engine and file names.**
- The engine name comes from 4nec2's settings. The *Generate* dialog shows
  `[Nec2dXS1k5]` for decks of 9 to 41 segments.
- File names are truncated to DOS 8.3 form: `dip_sweep.nec` ran as
  `..\out\dip_swee.inp` / `.out`, and 4nec2 renamed the output to `dip_sweep.out`
  afterwards and deleted the `.inp`.

**4nec2 checks geometry before it calls the kernel.** Overlaid wires stop the run
in 4nec2 itself, with no kernel call.

### The `.inp` 4nec2 writes

- **Preprocessed in full.** `SY` symbols are evaluated, `#12` wire gauges become
  radii, `50%` segment references are resolved, and comments are stripped
  (`Example3`).
- **`RP` is substituted.** In *Far Field pattern* mode, 4nec2's own `RP` replaces
  the deck's, for example `RP 0 37 73 1003 -180 0 5 5`.
- **`EN` is usually dropped.** "Use original file" passes the deck through
  unchanged except for removing the final `EN`, but a frequency-sweep run kept it.
  The kernel treats end-of-file as `EN` and echoes a synthesized
  `DATA CARD NO. n EN` that repeats the previous card's fields. nec2c refuses
  such a deck.
- **`GN 2` needs no grid file.** No `som2d.tmp` or grid file was written; this
  merged kernel computes the Sommerfeld ground itself.

### The `.out` 4nec2 reads

**4nec2 rejects nec2c's valid output** for the same deck with "NEC error?". nec2c
uses a different layout (dashed banners, other column titles), so 4nec2's parser
is tied to the original Fortran NEC-2D layout that nec2dxs prints.

**Removal ablation.** The kernel's `.out` was loaded with one part removed or
changed at a time:

| Removed | Effect in 4nec2 |
|:--|:--|
| banner text changed | none |
| `MATRIX TIMING` | none |
| `STRUCTURE IMPEDANCE LOADING` and `ANTENNA ENVIRONMENT` | none visible (4nec2 does show the environment text verbatim in its Environment box) |
| `RADIATION PATTERNS` | no pattern; all else intact |
| `CURRENTS AND LOCATION` | the input current is misread |
| `POWER BUDGET` | voltage and current mis-scaled (4nec2 normalises to 100 W from it) |
| the `DATA CARD` echo | impedance lost and the pattern line count wrong (4nec2 reads the `RP` parameters from the echo) |
| `STRUCTURE SPECIFICATION` and `SEGMENTATION DATA` | everything lost |
| the final `RUN TIME` line | "NEC error ?" |

**Insertion ablation.** One free-text line was inserted at a time: at the top,
inside `COMMENTS`, before `ANTENNA INPUT PARAMETERS`, inside `POWER BUDGET`,
after the `EN` echo, before `RUN TIME`, and after `RUN TIME`. 4nec2 parsed every
variant in full, with the same impedance and the same 5329 pattern lines.

## Binary-name contract

- [x] Confirm accepted executable-name variants and case-sensitivity behavior used by host tooling.
	- 2026-05-30: CLI detection contract now locks known stem variants case-insensitively (`nec2dxs500`, `nec2dxs1K5`, `nec2dxs3k0`, `nec2dxs5k0`, `nec2dxs8k0`, `nec2dxs11k`) and adds false-positive guard for unknown `nec2dxs*` names.
- [x] Map each variant to expected segment or model-size bands.
	- 2026-05-30 mapping (from artifact naming): `nec2dxs500` -> 500; `nec2dxs1K5` -> 1500; `nec2dxs3k0` -> 3000; `nec2dxs5k0` -> 5000; `nec2dxs8k0` -> 8000; `nec2dxs11k` -> 11000.

## Installation and replacement workflow

- [x] Document required copy/replace steps in a standard 4nec2 Windows installation.
	- 2026-05-30: documented in `docs/par011-dropin-contract.md` section "Windows install and rollback procedure (deterministic)".
- [x] Confirm whether side-by-side binaries are selected automatically by host tooling.
	- 2026-10-16 (Wine): **yes, by segment count**: decks of 9 to 84 segments ran `Nec2dXS1k5`, a 2001-segment deck `Nec2dXS3k0` (process trace). A drop-in therefore replaces all six names (`docs/4nec2-drop-in.md`).
- [x] Confirm rollback path for restoring original binaries.
	- 2026-05-30: deterministic rollback path documented in `docs/par011-dropin-contract.md`.

## Invocation and process contract

- [x] Capture argv shape from host-to-kernel execution.
	- 2026-10-15 (Wine): no arguments; the kernel is run as `Nec2dXS1k5.exe < nec2d.tmp` by `4nec2.bat`.
- [x] Capture expected working-directory assumptions.
	- 2026-10-15 (Wine): cwd is 4nec2's `exe` directory; the paths on stdin are relative to it.
- [x] Capture stdin/stdout/stderr usage and parsing assumptions.
	- 2026-10-15 (Wine): stdin carries two CRLF lines (input, output path); stdout carries prompts 4nec2 ignores; the results are read from the output file only.
- [x] Capture expected exit-code semantics for success and common failure classes.
	- 2026-10-15 (Wine): the exit code is 0 even on a deck error; success is the output file's final ` RUN TIME =` line.

Phase-0 template status:

- 2026-05-30: process-contract capture template published in `docs/par011-dropin-contract.md`; observational capture values remain pending.

## Filesystem side effects

- [x] Enumerate all temporary files written/read by the external-kernel contract.
	- 2026-10-15 (Wine): `exe\nec2d.tmp` (two paths), `out\NAME.inp` and `out\NAME.out` with 8.3 names; nothing else, `GN 2` included.
- [x] Record overwrite/cleanup lifecycle expectations.
	- 2026-10-15 (Wine): each run rewrites `nec2d.tmp`, the `.inp` and the `.out`; after a frequency sweep 4nec2 renames the 8.3 `.out` to the full name and deletes the `.inp`.
- [x] Record behavior when output files already exist.
	- 2026-10-15 (Wine): an existing `.out` is overwritten without a prompt.

## Dependency surface

- [x] Confirm runtime DLL dependencies, if any.
	- 2026-10-15: the bundled `nec2dxs1K5.exe` imports only `KERNEL32.dll` and `msvcrt.dll` (`objdump -p`).
- [ ] Confirm search-path/loader assumptions for portable and installed modes.

## Regression fixtures

- [ ] Archive representative call traces for each binary-name variant.
	- 2026-10-15/16 (Wine): `Nec2dXS1k5` and `Nec2dXS3k0` seen to run; the other four not.
- [x] Archive canonical input/output pairs suitable for CI compatibility tests.
	- 2026-10-15 (Wine): 15 pairs in `apps/nec-cli/tests/fixtures/par011/`, each `.inp` written by 4nec2 itself.

## Benchmark protocol

- [ ] Define apples-to-apples benchmark method against single-thread legacy kernels.
- [ ] Include segment-band splits aligned to binary-name variants.
- [ ] Capture machine metadata and run settings for reproducibility.

## Current implementation gap summary

- Filename-steered detection is present with an explicit known-name matrix (`nec2dxs500`, `nec2dxs1K5`, `nec2dxs3k0`, `nec2dxs5k0`, `nec2dxs8k0`, `nec2dxs11k`) plus `4nec2` naming fallback, and is covered by CLI unit tests.
- Execution-mode steering is currently limited to internal mode defaults and diagnostics.
- 2026-10-16: the external-kernel invocation and file contract are implemented (`fnec nec2d-kernel`, the NEC-2D report as `--output-format nec2d`), the filename steering is retired, and a Windows build (`scripts/build-windows-kernel.sh`) installed under all six names runs inside 4nec2 under Wine: 4nec2 reads fnec's report for every printable fixture deck and shows fnec's reason for a refused one (`scripts/par011-wine-e2e.sh`; its negative control, the original kernels, fails as it must). Real Windows is untested.
- Documentation-contract capture is complete through phase 2; remaining unchecked items require external evidence capture or implementation work rather than more contract-only docs.

## Docs-only phased implementation plan

This plan intentionally scopes to documentation and acceptance criteria only. No harness skeleton work is included in this phase.

### Phase 0: Contract capture and freeze

Deliverables:

- Capture Windows installation/replacement contract as step-by-step procedure.
- Capture binary-name matrix with expected size-band intent.
- Capture process contract template (argv, cwd, stdio, exit codes, timeout semantics).

Acceptance tests:

- [x] `AT-PAR011-0001`: document includes exact executable-name matrix and source evidence links.
- [x] `AT-PAR011-0002`: document includes deterministic install rollback procedure.
- [x] `AT-PAR011-0003`: process-contract template reviewed and checked into docs.

### Phase 1: Compatibility behavior specification

Deliverables:

- Define expected behavior for filename-steering defaults and explicit `--exec` override precedence.
- Define warning/diagnostic contract for drop-in profile activation.
- Define expected file side effects and lifecycle constraints.

Acceptance tests:

- [x] `AT-PAR011-0101`: docs specify precedence rule: explicit `--exec` always wins.
- [x] `AT-PAR011-0102`: docs specify warning text classes for steered vs preserved-explicit paths.
- [x] `AT-PAR011-0103`: docs enumerate side-effect files and cleanup expectations.

### Phase 2: Validation and benchmark specification

Deliverables:

- Define compatibility fixture format for external call traces.
- Define benchmark protocol for comparing replacement kernel throughput vs legacy single-thread baseline.
- Define required metadata capture for reproducible benchmark records.

Acceptance tests:

- [x] `AT-PAR011-0201`: fixture schema documented with at least one concrete example record.
- [x] `AT-PAR011-0202`: benchmark protocol includes segment-band and machine metadata requirements.
- [x] `AT-PAR011-0203`: success criteria for parity and performance are explicit and measurable.

### Postponed scope

- Harness skeleton for executing compatibility fixtures is explicitly postponed (user-requested defer of option 3).

## Remaining evidence-only queue

The unresolved checklist items above should be collected in one bundled external-evidence pass where practical, not as separate doc-only PRs.

Recommended bundle contents:

1. side-by-side selection behavior observation in a real 4nec2 install
2. process-contract capture (`argv`, `cwd`, stdio, exit codes, timeout semantics)
3. filesystem side-effect capture
4. dependency-surface confirmation (DLL/runtime and loader assumptions)
5. representative fixtures and first benchmark records

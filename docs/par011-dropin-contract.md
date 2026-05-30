---
project: fnec-rust
doc: docs/par011-dropin-contract.md
status: living
last_updated: 2026-05-30
---

# PAR-011 4nec2 Drop-In Contract (Phase 0)

This document captures the phase-0 contract for PAR-011:

- deterministic Windows install and rollback procedure for external-kernel replacement
- explicit binary-name matrix and intended segment-band mapping
- process-contract template for host-to-kernel invocation capture

This is a contract and evidence-capture spec only. It does not claim full drop-in parity.

## Binary-name matrix (contract)

| Binary name | Intended segment band |
|:------------|:----------------------|
| `nec2dxs500.exe` | up to 500 segments |
| `nec2dxs1K5.exe` | up to 1500 segments |
| `nec2dxs3k0.exe` | up to 3000 segments |
| `nec2dxs5k0.exe` | up to 5000 segments |
| `nec2dxs8k0.exe` | up to 8000 segments |
| `nec2dxs11k.exe` | up to 11000 segments |

## Windows install and rollback procedure (deterministic)

Assumptions:

- 4nec2 installation path has an `EXE` folder (default pattern: `C:\\4NEC2\\EXE`).
- Operator has write permission in the install folder.
- Replacement kernel executable is prepared and validated independently.

### Install/replace steps

1. Close 4nec2.
2. In the 4nec2 `EXE` folder, create a timestamped backup directory:
   - `backup-YYYYMMDD-HHMMSS`.
3. Copy each existing `nec2dxs*.exe` candidate into the backup directory.
4. Copy replacement executable(s) into the `EXE` folder using the exact target filename(s) from the matrix above.
5. Record a replacement manifest file in the backup directory:
   - target filename
   - replacement file SHA256
   - original file SHA256
   - timestamp
6. Start 4nec2 and select the intended `Nec2dXS*.exe` kernel in 4nec2 settings.
7. Run a known reference deck and capture run metadata (selected kernel filename, exit status, and output artifacts).

### Rollback steps

1. Close 4nec2.
2. Select the backup directory associated with the active replacement rollout.
3. Copy each original `nec2dxs*.exe` file from backup back into `EXE`, overwriting replacement files.
4. Reopen 4nec2 and reselect the previously used kernel setting if needed.
5. Run the same reference deck used in step 7 above and confirm behavior has returned to pre-replacement baseline.
6. Record rollback completion timestamp and operator note in the same manifest.

## Process-contract template (capture form)

Use this template when observing host (4nec2) to kernel-process behavior.

| Field | Captured value | Notes |
|:------|:---------------|:------|
| Host tool version | TBD | Example: 4nec2 version/build string |
| Kernel binary name | TBD | One of `nec2dxs*.exe` |
| Full command line (`argv`) | TBD | Include all flags and positional arguments |
| Working directory (`cwd`) | TBD | Record absolute path |
| Environment variables | TBD | Include only variables observed as relevant |
| stdin behavior | TBD | Whether stdin is used, and expected format |
| stdout behavior | TBD | Stream format and parse assumptions |
| stderr behavior | TBD | Diagnostic/error parse assumptions |
| Exit code success | TBD | Success code(s) expected by host |
| Exit code failure classes | TBD | Distinguish parse, runtime, timeout, IO |
| Timeout behavior | TBD | Host timeout threshold and handling |
| Input file side effects | TBD | Files read/created/updated |
| Output file side effects | TBD | Files produced and overwrite rules |

## Acceptance-test mapping (phase 0)

- `AT-PAR011-0001`: satisfied by matrix and evidence links in `docs/par011-dropin-evidence-memo.md`.
- `AT-PAR011-0002`: satisfied by deterministic install and rollback procedure in this document.
- `AT-PAR011-0003`: satisfied by process-contract template in this document.

## Phase 1 compatibility behavior specification

This section freezes the expected behavior of the current filename-steered compatibility scaffold. It documents the behavior that exists today; it does not claim full external-kernel parity.

### Execution-mode precedence rule

Precedence is strict:

1. An explicit `--exec <cpu|hybrid|gpu>` flag always wins.
2. If `--exec` is omitted and the drop-in profile is active, execution defaults to `hybrid`.
3. If `--exec` is omitted and the native profile is active, the normal startup auto-probe policy applies.

Implication:

- drop-in profile activation may change the default execution path
- drop-in profile activation must not override an explicit operator choice

### Warning and diagnostic contract

When the drop-in profile is active, stderr warning classes are fixed to two cases:

1. Steered-default case:
   - `warning: 4nec2 drop-in compatibility profile detected by binary name; default execution path steered to exec=hybrid`
2. Explicit-preserved case:
   - `warning: 4nec2 drop-in compatibility profile detected by binary name; preserving explicit --exec=<value>`

Diagnostic expectations:

- the warning is emitted on stderr, not stdout
- the normal `diag:` line still reflects the effective execution mode (`exec=hybrid`, `exec=cpu`, or `exec=gpu(cpu-fallback)`)
- no additional drop-in-specific stdout markers are introduced in the current scaffold

### File side-effect and lifecycle constraints

Current scaffold expectation:

- filename steering changes only in-process execution-mode selection and emitted diagnostics
- it does not add new temporary files, cache files, manifests, or host-visible compatibility artifacts
- it does not alter the existing report-stream split: human/machine report content remains on stdout; warnings and benchmark/diagnostic records remain on stderr

Expected side-effect constraints for the current scaffold:

1. Input deck handling:
   - read the provided deck path only
   - do not rewrite the input deck in place
2. Output files:
   - no new compatibility-profile-specific output files are created
   - any future file-based compatibility layer must define overwrite rules explicitly before implementation
3. Cleanup:
   - no new temporary compatibility files implies no additional cleanup phase in the current scaffold
4. Existing-file behavior:
   - because no new drop-in-specific files are created, existing unrelated files in the working directory must remain untouched by profile activation alone

## Acceptance-test mapping (phase 1)

- `AT-PAR011-0101`: satisfied by the execution-mode precedence rule above.
- `AT-PAR011-0102`: satisfied by the fixed warning/diagnostic classes above.
- `AT-PAR011-0103`: satisfied by the file side-effect and lifecycle constraints above.

---
project: fnec-rust
doc: docs/project/path-inventory.md
status: living
last_updated: 2026-08-23
---

# Path inventory for cross-cutting concerns

A cross-cutting concern — a validation, a limit, a diagnostic — is only as good as
the *least* covered path that reaches it. This file enumerates, for each concern,
**every production execution path**, traced top-down from a shipped artifact, and
what proves the concern reaches it.

Shipped artifacts: `fnec` (CLI), `fnec-gui` (GUI), `fnec_py` (Python extension
module), and `fnec worker --stdio` (the remote solver behind `--hosts`). The last
one is the easiest to forget, and duly was — see C1.

**Rule this file exists to enforce:** *"covers all paths"* may not be written in any
project artifact without a per-path list here or a single-seam proof, and the
evidence must exercise the production entry point, not a convenience wrapper.

Gaps are not hidden here. Each links to its findings-ledger ID.

---

## C1 — Pre-solve deck validation (`nec_solver::validate`)

Rejects geometry outside the solver's supported class (crossing wires, a source on
a degenerate segment, a wire reaching an active ground) and collects the caveats.

| # | Path | Entry point | Covered | Evidence |
|:--|:-----|:------------|:--------|:---------|
| 1 | CLI, local solve | `nec-cli/src/main.rs` `main()` | yes | `apps/nec-cli/tests/geometry_diagnostics.rs` |
| 2 | GUI, impedance | `nec-gui` `solve_deck_str` | yes | `gui_refuses_the_geometry_the_cli_refuses` |
| 3 | GUI, sweep | `nec-gui` `SweepJob::prepare` | yes | `every_gui_solve_path_applies_the_same_rejection` |
| 4 | GUI, currents + pattern | `nec-gui` `solve_for_currents` | yes | `every_gui_solve_path_applies_the_same_rejection` |
| 5 | Python bindings | `fnec_py.solve_deck_str` / `sweep_deck_str` | yes | `bindings/fnec_py/tests/test_smoke.py` |
| 6 | CLI, distributed | `nec-cli/src/main.rs` `run_distributed_solve` | **NO** | gap — [FND-013](findings-ledger.md) |
| 7 | Remote worker | `nec_worker/src/solve.rs` `solve_task` | **NO** | gap — [FND-013](findings-ledger.md) |

Paths 6 and 7 are the same request seen from two ends, and **neither** validates:
`--hosts` returns from `main()` before the validation block, and the worker goes
from `build_geometry` straight to the solve. A deck the CLI refuses locally is
dispatched to every worker and solved.

Demonstrated rather than read off the source — the local path names the geometry
error, the distributed path never mentions it:

```
$ fnec crossing.nec
error: unsupported intersecting-wire geometry between tag 1 seg 6 and tag 2 seg 6…

$ fnec --hosts hosts.toml crossing.nec
error: worker pool is empty — no workers available      # geometry never checked
```

## C2 — Negative-resistance tripwire

A passive antenna cannot have `Re(Z) < 0`; when one is reported the result is
unphysical and the user must be told.

| # | Path | Covered | Evidence |
|:--|:-----|:--------|:---------|
| 1 | CLI, `--solver hallen` | yes | `apps/nec-cli/tests/junction_feedpoint.rs` |
| 2 | CLI, `--solver mpie` | yes | armed as a standing tripwire in #365 |
| 3 | CLI, pulse / continuity / sinusoidal | n/a | deliberately skipped: the current-source corpus has documented negative-`R` values |
| 4 | GUI | **NO** | gap — [FND-014](findings-ledger.md) |
| 5 | Python bindings | **NO** | gap — [FND-014](findings-ledger.md) |
| 6 | Remote worker | **NO** | gap — [FND-014](findings-ledger.md) |

The check is *post*-solve, so it is not part of `validate::diagnose`, which is what
the GUI and the bindings adopted in #369/#370. That is why wiring the pre-solve seam
did not carry this one with it.

## C3 — GPU-resident solve accuracy gate

An f32 solve that has not converged must not be reported as a result.

| # | Path | Covered | Evidence |
|:--|:-----|:--------|:---------|
| 1 | `solve_hallen_gpu_resident` | yes | `gpu_resident_never_reports_a_diverged_solve` |
| 2 | `fill_zmatrix_wgpu` → CPU solve | n/a | f32 fill, f64 solve; bounded by the 2 Ω GPU-path test instead |
| 3 | RP far-field kernels | n/a | no linear solve involved |

Single seam: every GPU-resident solve returns through one function, and the gate is
in it. There is no second way to reach that kernel.

## C4 — Ground-model diagnostics

| # | Path | Covered | Evidence |
|:--|:-----|:--------|:---------|
| 1 | CLI low-antenna-over-finite-ground | yes | `apps/nec-cli/tests/sommerfeld_ground_cli.rs` |
| 2 | CLI declined Sommerfeld request | yes | `declined_sommerfeld_geometry_is_reported_not_silent` |
| 3 | GUI / Python low-ground | yes | carried by `validate::diagnose` |
| 4 | GUI / Python declined Sommerfeld | n/a | neither exposes `--ground-solver`, so the request cannot be made |

## C5 — Load / TL / NT builder warnings

Malformed `LD`, `TL` and `NT` cards are skipped; the user must learn they were.

| # | Path | Covered | Evidence |
|:--|:-----|:--------|:---------|
| 1 | CLI | yes | printed in `solve_session.rs` |
| 2 | GUI | yes | collected in `validate_deck` (#369) |
| 3 | Python bindings | yes | raised as `UserWarning` (#370) |
| 4 | Remote worker | **NO** | gap — [FND-015](findings-ledger.md) |
| 5 | GUI / worker `NT` stamps | **NO** | `build_nt_stamps` is not called at all off the CLI path — [FND-015](findings-ledger.md) |

---

## Maintaining this

`scripts/check-path-inventory.py` verifies the file cannot rot into fiction: every
cited test file must exist in the tree, and every row marked as a gap must cite a
findings-ledger ID that is really in the ledger. It runs in CI.

It checks references, not coverage — whether a cited test genuinely exercises its
path is a question for the test, not for a grep.

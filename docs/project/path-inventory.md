---
project: fnec-rust
doc: docs/project/path-inventory.md
status: living
last_updated: 2026-08-24
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
| 6 | CLI, distributed | `nec-cli/src/main.rs`, above the `--hosts` branch | yes | `distributed_run_refuses_geometry_before_contacting_any_host`; `the_distributed_caveats_come_from_the_shared_producer`; `a_non_hallen_distributed_run_gets_no_hallen_caveats` |
| 7 | Remote worker | `nec_worker/src/solve.rs` `solve_deck_at_frequency_with_exec` | errors yes, caveats via the controller † | `worker_refuses_geometry_the_cli_refuses` |

† This concern has two halves — reject the unsupported class, **and** collect the
caveats. The worker does the first; the second is done for it by the controller,
which holds the deck, the geometry, the ground model and the frequencies before it
dispatches anything (`distributed_pre_solve_caveats`, FND-020).

That split is deliberate rather than incidental. A caveat computed worker-side
goes silent against an older worker, because a worker is a separately installed
binary — so anything the controller can derive for itself belongs to the
controller, and only what the worker's own solve *did* travels on the wire (the
stamp warnings of FND-026). Row 7 is therefore covered, but not by the worker.

Both paths call one producer, `validate::hallen_geometry_caveats`, so a caveat
added there reaches both by construction — demonstrated by adding a probe caveat to
the producer and watching it appear on the local *and* distributed routes of the
real binary, with no call site touched.

That is a stronger claim than the one this row made first. The original gate
compared the distributed output against a hand-copied list of the same three calls,
which would have stayed green if a *fourth* caveat were added — the very failure it
claimed to prevent. Caveats that depend on options one frontend owns (a declined
`--ground-solver sommerfeld`) deliberately stay with that frontend.

Paths 6 and 7 are the same request seen from two ends, and **neither** validated
until #390 (FND-013): `--hosts` returned from `main()` before the validation block,
and the worker went from `build_geometry` straight to the solve, so a deck the CLI
refuses locally was dispatched to every worker and solved.

The fix is one shared block **above** the `--hosts` branch rather than a copy
inside `run_distributed_solve`. Placement is load-bearing, not stylistic:
`WorkerPool` spawns an SSH process per host the moment it is constructed, so a
check inside the distributed function would connect to every host before noticing
the deck was never solvable.

Path 7 is not redundant with path 6. The worker is a **separately installed
binary**, reached at whatever `binary_path` the hosts file names, so it may be a
different fnec version with a different supported class — a controller can never
speak for it. `run_worker_stdio` is also public API fed by arbitrary stdin. Path 7
is the authoritative end; path 6 is fail-fast UX.

## C2 — Negative-resistance tripwire

A passive antenna cannot have `Re(Z) < 0`; when one is reported the result is
unphysical and the user must be told.

| # | Path | Covered | Evidence |
|:--|:-----|:--------|:---------|
| 1 | CLI, `--solver hallen` | yes | `apps/nec-cli/tests/junction_feedpoint.rs` |
| 2 | CLI, `--solver mpie` | yes | armed as a standing tripwire in #365 |
| 3 | CLI, pulse / continuity / sinusoidal | n/a | deliberately skipped: the current-source corpus has documented negative-`R` values |
| 4 | GUI, single solve | yes | `a_negative_resistance_solve_carries_a_caveat` |
| 5 | GUI, sweep | yes | `the_sweep_caveat_is_one_line_for_the_whole_sweep` |
| 6 | Python bindings | yes | `test_a_negative_resistance_deck_raises_a_warning` |
| 7 | Remote worker (`--hosts`) | yes | `a_negative_distributed_result_earns_a_caveat_naming_the_real_feedpoint` |

C2 demonstrated before the fix: a 3-segment 40 m wire reports `Re(Z) = -162.547 Ω`;
the CLI warned, `fnec_py` raised nothing. The same build did raise a `UserWarning`
for a low-ground deck, so the channel worked — the check simply was not on it.

The check is *post*-solve, so it is deliberately **not** part of `validate::diagnose`,
which the GUI and the bindings adopted in #369/#370 and which is contracted to run
without a matrix. That is why wiring the pre-solve seam did not carry this one with
it, and why closing the gap needed its own seam,
`validate::negative_resistance_warning`, called after each solve.

Row 7 is covered **controller-side**, not in the worker. A worker is a separately
installed binary, so a check that lived only there would go silent against an older
worker — reproducing the finding under version skew. The controller already has the
impedance and the deck, so it covers every worker ever built.

Rows 4 and 5 are one check with two presentations. The single solve appends to the
`SolveResult.warnings` the panel already renders; the sweep gets **one aggregate
line** naming how many points went negative, because the cause is a property of the
geometry and does not vary across frequency — a per-point warnings field would
repeat one diagnosis up to `MAX_SWEEP_POINTS` times while restating values the point
already carries.

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
| 3 | GUI / Python **single solve** low-ground | yes | carried by `validate::diagnose` |
| 4 | GUI **sweep** low-ground | **NO** | gap — [FND-042](findings-ledger.md); evaluated at the `FR` card's frequency, not the swept range |
| 5 | CLI distributed low-ground | yes | `the_low_ground_check_uses_the_worst_case_frequency_not_the_first` |
| 6 | GUI / Python declined Sommerfeld | n/a | neither exposes `--ground-solver`, so the request cannot be made |

Row 3's "yes" was unqualified until #399 split it. The check is frequency-dependent,
and a *sweep* is a different frequency from the deck's `FR` card — so covering the
single solve says nothing about the sweep, which is row 4.

## C5 — Load / TL / NT builder warnings

Malformed `LD`, `TL` and `NT` cards are skipped; the user must learn they were.

| # | Path | Covered | Evidence |
|:--|:-----|:--------|:---------|
| 1 | CLI | yes | printed in `solve_session.rs` |
| 2 | GUI | yes | collected in `validate_deck` (#369) |
| 3 | Python bindings | yes | raised as `UserWarning` (#370) |
| 4 | Remote worker | yes | `a_skipped_card_warning_survives_the_wire_to_a_real_worker` (subprocess boundary); `a_malformed_card_the_worker_skips_is_reported_not_swallowed`; wire compatibility both ways. The controller's `eprintln` itself is review-verified, not test-pinned |
| 5 | `NT` stamping — GUI, Python, worker | yes | all six assembly sites go through `build_deck_stamps`; `test_nt_deck_matches_the_corpus_reference` |

## C6 — Which `EX` card is the feedpoint

A deck's `EX` cards are not interchangeable. A plane wave has **no** feedpoint —
its tag and segment fields carry NTHETA and NPHI — while types 0 and 5 are delta
gaps and type 4 is a current source that is a feedpoint but is priced differently.
Every site that reports an impedance, or names where one came from, has to answer
this.

| # | Path | Covered | Evidence |
|:--|:-----|:--------|:---------|
| 1 | `build_hallen_rhs` (delta-gap RHS) | yes | `match … feedpoint_role()`, no wildcard; `unknown_ex_type_is_error` |
| 2 | `build_hallen_rhs_paths` (second loop) | yes | same match; same error channel |
| 3 | `apply_ex` (EFIE voltage vector) | yes | same match |
| 4 | Remote worker | yes | `a_type_5_voltage_source_is_a_feedpoint_here_as_it_is_everywhere_else` |
| 5 | CLI feedpoint rows | yes | `feedpoints`, keyed on the role; `dipole-ex4-freesp-51seg` corpus gate |
| 6 | GUI | yes | `a_plane_wave_is_not_read_as_the_feedpoint` |
| 7 | Python bindings | yes | `test_a_plane_wave_is_not_read_as_the_feedpoint` |
| 8 | CLI distributed diagnostic | yes | `the_caveat_names_the_same_feedpoint_the_worker_reported` |
| 9 | `validate::source_risk_geometry_error` | yes | `a_plane_wave_does_not_trigger_a_source_risk_rejection`; end-to-end `a_receive_only_deck_is_not_refused_for_a_source_it_does_not_have` |
| 10 | `validate::feedpoint_at_junction_warnings` | yes | `an_unrecognised_excitation_is_not_treated_as_a_junction_feedpoint` |
| 11 | MPIE session feedpoint | **NO** | gap — [FND-037](findings-ledger.md); `!is_plane_wave()` only, shielded by gates in another function |

Rows 1–3 keep their own loops rather than calling the reporting seam, and that is
the principled half of the split: building an RHS needs an `UnsupportedType` error
channel that naming a feedpoint does not. What they no longer keep is their own
*policy* — all three `match` on `ExcitationKind::feedpoint_role()` with no wildcard
arm, so a new excitation type breaks the build in three places rather than
defaulting into whichever branch was last.

The reporting seam needs no error channel for `Unknown`: every reporting site sits
strictly downstream of `build_excitation`/`build_hallen_rhs`, which reject an
unknown type before any current exists to report.

Rows 8 and 4 are the same call by construction now. That parity used to be a
comment asking two files to be kept in step, and they diverged twice inside a
single review.

---

## Maintaining this

`scripts/check-path-inventory.py` verifies the file cannot rot into fiction: every
cited test file must exist in the tree, and every row marked as a gap must cite a
findings-ledger ID that is really in the ledger. It runs in CI.

It checks references, not coverage — whether a cited test genuinely exercises its
path is a question for the test, not for a grep.

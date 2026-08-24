---
project: fnec-rust
doc: docs/project/findings-ledger.md
status: living
last_updated: 2026-08-24
---

# Findings ledger

Every discovery gets an ID here the moment it is found — not "to be reviewed
later". A finding that is noticed, discussed, and then quietly dropped is exactly
the thing this ledger exists to stop.

Before this file existed, discoveries scattered: one lived in a review document,
one in a changelog entry, one in a design doc, one only in a session recital of
"still open: X, Y, Z". Nothing was lost, but that was down to the recital rather
than to any structure.

## How to use it

- **ID**: `FND-NNN`, assigned in order, never reused. (`FIND-NNN` belongs to
  `docs/dev/reviews/review-260719.md` and is a different, closed series.)
- **Newest first.** New findings go at the top of the table.
- **State** is one of four, and only the first is non-terminal:

  | State | Meaning | Required |
  |:------|:--------|:---------|
  | `open` | found, not yet resolved | — |
  | `fixed` | resolved in the tree | the PR or commit that did it |
  | `deferred` | deliberately not now | an owner **and** where it is tracked |
  | `rejected` | will not act | a rationale |

- `scripts/check-findings-ledger.py` enforces the shape of every row: an ID, a
  known state, and the evidence that state requires. It runs in CI, so a
  `deferred` row with no owner or a `fixed` row with no reference fails the build
  rather than sitting here looking resolved.

An `open` row is not a failure — it is the point. What the process forbids is an
`open` row nobody can see.

## Ledger

| ID | Found | State | Finding | Evidence / owner |
|:---|:------|:------|:--------|:-----------------|
| FND-022 | 2026-08-24 | fixed | `check-path-inventory.py` only validated backticked names of 15+ characters, so the invented symbol `solve_task` sat in the path inventory and the ledger through two reviews without the gate noticing. Length was never what makes a name checkable — containing an underscore is. Fixing that exposed two more ways the check confirmed whatever it was given: it substring-matched, so a truncated name resolved to the real one it prefixes; and it searched `docs/` and `scripts/`, so a name resolved to its own mention in the file under check, or to the examples in the checker's own comment. | #391 — sabotage-verified against both `solve_task` and a truncated `build_nt_stamp` |
| FND-021 | 2026-08-24 | open | `build_hallen_rhs`'s non-`UnsupportedType` excitation errors map to `SolveError::ParseError` in the worker (`nec_worker/src/solve.rs`), so an `EX` referencing a missing segment crosses the wire labelled `parse_error` and sends the user to look at their deck's syntax. Same mislabel class the FND-013 fix avoided for geometry. Pre-existing. | found by fable's diff review of #390 |
| FND-020 | 2026-08-24 | open | The distributed path collects only half of concern C1: it rejects the unsupported geometry class but emits only the deferred-ground and unsupported-topology caveats, not `low_finite_ground_warning` or `feedpoint_at_junction_warnings`. A dipole at 0.03 λ over `GN 2` run via `--hosts` returns numbers with no low-ground caveat where every other frontend warns. | found by fable's diff review of #390; `docs/project/path-inventory.md` C1 † |
| FND-019 | 2026-08-24 | open | `WorkerSolverConfig::ground_model` is sent as `"none"` by the controller and never read by the worker's solve, which derives the ground from the deck (`ground_model_from_deck`). It does feed the result-cache key, so it is not wholly dead — but a protocol field that looks like it selects a ground model and does not is the same trap that produced FND-013. | found by fable's review of the FND-013 design |
| FND-018 | 2026-08-24 | open | `fnec --hosts h.toml --solver mpie deck.nec` connects to every host (5 s SSH timeout each) before every task fails `UnsupportedConfig` — the worker accepts only `basis == "hallen"`. The basis should be rejected controller-side before any SSH spawn, next to the FND-013 geometry check. | found by fable's review of the FND-013 design |
| FND-017 | 2026-08-24 | fixed | Six `nec-cli` integration-test files wrote uniquely-named decks to the system temp directory and never removed them, while the other eighteen did. One session of repeated `cargo test --workspace` runs left **437** stray `fnec-*` files in `/tmp` — enough to break the sandbox this agent's shell runs in, which is how it was found. Now a shared `common::TempDeck` RAII guard that also survives a panicking test, which the pre-existing trailing-`remove_file` convention does not. Residual, bounded and deliberate: 6 GUI-crate fixtures use fixed names, so they overwrite rather than accumulate. | #389 — a full `cargo test --workspace` now leaves 6 fixed-name files, down from ~437 unique ones |
| FND-016 | 2026-08-24 | open | `apps/nec-cli/Cargo.toml:21` declares `nec_project` as a dependency that no CLI source file imports — the binary links and the SBOM carries a crate it never uses. Either wire it (which is what FND-006 asks for) or drop the declaration. | found while correcting FND-006; `cargo` would catch it under the `unused_crate_dependencies` lint |
| FND-015 | 2026-08-23 | open | `build_nt_stamps` is called **only** from `apps/nec-cli/src/solve_session.rs`, so `NT` cards are ignored outright by the GUI, the Python bindings and the worker. **Demonstrated: the same deck gives different impedances per frontend** — `corpus/dipole-nt-tl-equiv-freesp-51seg.nec` solves to 70.633 + j14.009 Ω on the CLI (NT stamped) and 74.243 + j13.900 Ω via `fnec_py` (NT ignored — identical to the plain dipole), 3.6 Ω / 5 % apart. The worker separately discards `_load_warnings` / `_tl_warnings`. A malformed `NT` that the CLI reports ("NT card has 8 fields; expected 10") is silent everywhere else. | demonstrated 2026-08-24; found by the C5 path inventory |
| FND-014 | 2026-08-23 | open | The negative-resistance tripwire is CLI-only. The check is post-solve, so adopting `validate::diagnose` in #369/#370 did not carry it. **Demonstrated:** a 3-segment 40 m wire returns `Re(Z) = -162.547 Ω` — physically impossible for a passive antenna — where the CLI warns and `fnec_py` raises **zero** warnings. Positive control: the same build *does* raise a `UserWarning` for a low-antenna-over-finite-ground deck, so the caveat channel works and this one simply is not on it. | demonstrated 2026-08-24; found by the C2 path inventory |
| FND-013 | 2026-08-23 | fixed | The distributed path skipped pre-solve validation at **both** ends: `--hosts` returned from `main()` before the validation block, and `nec_worker::solve_deck_at_frequency_with_exec` went from `build_geometry` straight to the solve. A deck the CLI refuses locally was dispatched to every worker and solved — demonstrated: the worker returned 49.53 − j173.05 Ω for crossing wires. | #390 — one shared check hoisted **above** the `--hosts` branch (placement is load-bearing: `WorkerPool` spawns SSH on construction) plus an independent worker-side guard, since the worker is a separately installed binary a controller cannot speak for |
| FND-012 | 2026-08-23 | open | `gpu_kernels` CPU-emulation scaffold still exists, so a path can report CPU time under a GPU-sounding name. Roadmap: "retire or realize". | owner: unassigned; tracked in `docs/roadmap.md` Phase 6 |
| FND-011 | 2026-08-23 | open | `--exec gpu` is not dispatched through the SSH worker pool, so GPU-capable remote nodes never solve on their GPU. | owner: unassigned; tracked in `docs/roadmap.md` Phase 6 |
| FND-010 | 2026-08-23 | open | No ROCm or SYCL backend validation beyond the wgpu Vulkan path, and no dated "not yet" recorded either. | owner: unassigned; tracked in `docs/roadmap.md` Phase 6 |
| FND-009 | 2026-08-23 | open | The GPU-resident dense solve never beats the CPU at any tested size (0.04×–0.48×) and is declined by the accuracy gate at N=512. Cause is structural: the LU dispatches one workgroup, so it runs on a single compute unit and more GPU hardware cannot help. Recommendation to treat it as not-recommended is recorded but not acted on. | measured in #384; `docs/ph7-chk-003-gpu-resident-solve.md` § Performance |
| FND-008 | 2026-08-23 | open | Real-hardware GPU benchmark evidence exists only for an **integrated** adapter (RADV RENOIR). The roadmap asks for at least one **discrete** GPU; that is still unmet, and the PH7-CHK-003 results were previously headed "real discrete GPU". | heading corrected in #384; roadmap item marked partial |
| FND-007 | 2026-08-23 | open | The GUI is Hallén-only: no solver picker. Decks needing `--solver mpie` are warned about and told to use the CLI instead of being solvable in place. | owner: unassigned; surfaced by #369, #381 |
| FND-006 | 2026-08-23 | open | Roadmap `GAP-015` is marked Done citing only `nec_project`'s library functions and their round-trip tests, but its own acceptance criterion names "explicit CLI/API entry points", which do not exist. The library half is delivered; the frontend half the criterion asks for is not. | roadmap row corrected from Done to Partial on 2026-08-24; dispositioned in #377, `docs/dev/reviews/review-260719.md` § Dispositions. Stays open: the CLI/API entry points the criterion names still do not exist |
| FND-005 | 2026-08-23 | deferred | Corpus per-case provenance records the workspace version *under development* at the commit, which is not necessarily a released build, and does not distinguish a case being added from its values being regenerated. | owner: developer; acceptable limit stated in `corpus/reference-results.json` `provenance_note` and enforced fresh by CI |
| FND-004 | 2026-08-23 | deferred | DCIM Phase 3 (Rust port of the validated Python prototype) cannot proceed: nec2c / xnec2c / EZNEC / 4nec2 are unavailable on this machine, so new geometries cannot be gated against an external oracle. | owner: developer (restoring tooling); `docs/sommerfeld-level2-scope.md` |
| FND-003 | 2026-08-23 | rejected | Three extension traits (`DeckPostProcessor`, `ResultFilter`, `ReportSection`) have no production consumers. | Deliberate plugin-API surface per `docs/plugin-api-design.md`. Kept, but labelled "future-facing API, unused today" rather than "consumed" — the review's own refutation was wrong, since `render_text_report_with_sections` has no production caller either. #377 |
| FND-002 | 2026-08-23 | fixed | `⚠` (U+26A0) has no glyph in iced's default font and rendered as a tofu box in every GUI caveat — pre-existing, and propagated to the new caveats strip. Only a visual check could see it; the tests assert on the text, which was always correct. | #383 |
| FND-001 | 2026-08-23 | fixed | `exec_modes` drop-in alias paths were keyed on `(name, nanoseconds)`, but six matrix tests loop over one shared alias-name list in parallel — two threads collide and `fs::copy` fails with `ETXTBSY`. Took down a release-branch coverage run. | #379 |

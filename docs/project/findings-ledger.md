---
project: fnec-rust
doc: docs/project/findings-ledger.md
status: living
last_updated: 2026-08-23
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
| FND-012 | 2026-08-23 | open | `gpu_kernels` CPU-emulation scaffold still exists, so a path can report CPU time under a GPU-sounding name. Roadmap: "retire or realize". | owner: unassigned; tracked in `docs/roadmap.md` Phase 6 |
| FND-011 | 2026-08-23 | open | `--exec gpu` is not dispatched through the SSH worker pool, so GPU-capable remote nodes never solve on their GPU. | owner: unassigned; tracked in `docs/roadmap.md` Phase 6 |
| FND-010 | 2026-08-23 | open | No ROCm or SYCL backend validation beyond the wgpu Vulkan path, and no dated "not yet" recorded either. | owner: unassigned; tracked in `docs/roadmap.md` Phase 6 |
| FND-009 | 2026-08-23 | open | The GPU-resident dense solve never beats the CPU at any tested size (0.04×–0.48×) and is declined by the accuracy gate at N=512. Cause is structural: the LU dispatches one workgroup, so it runs on a single compute unit and more GPU hardware cannot help. Recommendation to treat it as not-recommended is recorded but not acted on. | measured in #384; `docs/ph7-chk-003-gpu-resident-solve.md` § Performance |
| FND-008 | 2026-08-23 | open | Real-hardware GPU benchmark evidence exists only for an **integrated** adapter (RADV RENOIR). The roadmap asks for at least one **discrete** GPU; that is still unmet, and the PH7-CHK-003 results were previously headed "real discrete GPU". | heading corrected in #384; roadmap item marked partial |
| FND-007 | 2026-08-23 | open | The GUI is Hallén-only: no solver picker. Decks needing `--solver mpie` are warned about and told to use the CLI instead of being solvable in place. | owner: unassigned; surfaced by #369, #381 |
| FND-006 | 2026-08-23 | open | Roadmap `GAP-015` is marked Done citing only `nec_project`'s library functions and their round-trip tests, but its own acceptance criterion names "explicit CLI/API entry points", which do not exist. The library half is delivered; the frontend half the criterion asks for is not. | dispositioned in #377; `docs/dev/reviews/review-260719.md` § Dispositions |
| FND-005 | 2026-08-23 | deferred | Corpus per-case provenance records the workspace version *under development* at the commit, which is not necessarily a released build, and does not distinguish a case being added from its values being regenerated. | owner: developer; acceptable limit stated in `corpus/reference-results.json` `provenance_note` and enforced fresh by CI |
| FND-004 | 2026-08-23 | deferred | DCIM Phase 3 (Rust port of the validated Python prototype) cannot proceed: nec2c / xnec2c / EZNEC / 4nec2 are unavailable on this machine, so new geometries cannot be gated against an external oracle. | owner: developer (restoring tooling); `docs/sommerfeld-level2-scope.md` |
| FND-003 | 2026-08-23 | rejected | Three extension traits (`DeckPostProcessor`, `ResultFilter`, `ReportSection`) have no production consumers. | Deliberate plugin-API surface per `docs/plugin-api-design.md`. Kept, but labelled "future-facing API, unused today" rather than "consumed" — the review's own refutation was wrong, since `render_text_report_with_sections` has no production caller either. #377 |
| FND-002 | 2026-08-23 | fixed | `⚠` (U+26A0) has no glyph in iced's default font and rendered as a tofu box in every GUI caveat — pre-existing, and propagated to the new caveats strip. Only a visual check could see it; the tests assert on the text, which was always correct. | #383 |
| FND-001 | 2026-08-23 | fixed | `exec_modes` drop-in alias paths were keyed on `(name, nanoseconds)`, but six matrix tests loop over one shared alias-name list in parallel — two threads collide and `fs::copy` fails with `ETXTBSY`. Took down a release-branch coverage run. | #379 |

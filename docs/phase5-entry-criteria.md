---
project: fnec-rust
doc: docs/phase5-entry-criteria.md
status: living
last_updated: 2026-05-03
---

# Phase 5 Entry Criteria

This document records the measurable acceptance criteria that **must all be
met** before GPU acceleration work (Phase 5) begins.  Every criterion is
verifiable from existing tooling; none are vague intentions.

Phase 5 covers GPU acceleration from postprocessing through to the solver
kernel — see the Phase 5 section of [`docs/roadmap.md`](roadmap.md) for the
full goal list.

---

## Criterion 1 — CPU baseline benchmarks locked

**Rationale**: GPU work produces regressions that are invisible unless a
stable CPU reference exists.  The baseline must be captured on at least two
hardware targets so that single-machine variance does not mask real slowdowns.

### Required state

1. `docs/benchmarks.md` contains baseline timing for at least **two** of the
   three target tiers (workstation, T480, Pi5-class) across the full benchmark
   matrix (3 decks × 3 solvers × 3 exec modes = 81 rows per target).

2. Baseline CSVs have been committed or their SHA is recorded in
   `docs/benchmarks.md` so results are reproducible.

3. `scripts/pi-benchmark-compare.sh --fail-on-mode-drift` exits 0 against the
   most recent baseline CSV for each locked target.

### Current state — **MET**

Baselines are captured and documented in [`docs/benchmarks.md`](benchmarks.md):

| Target | Baseline CSV | Rows | Non-OK |
|:-------|:-------------|-----:|-------:|
| local-workstation | `tmp/local-baseline-20260427T111026Z.csv` | 81 | 0 |
| target-t480 | `tmp/t480-baseline-20260427T101204Z.csv` | 81 | 0 |
| target-pi5 | `tmp/pi5-baseline-20260427T101239Z.csv` | 81 | 0 |

Representative Hallen timings (ms, all exec modes averaged):

| Target | hallen | pulse | sinusoidal |
|:-------|-------:|------:|-----------:|
| local-workstation | 487.4 | 128.4 | 142.0 |
| target-t480 | 489.0 | 129.4 | 141.6 |
| target-pi5 | 934.2 | 228.1 | 253.4 |

Pi5 vs workstation ratio: ≈ 1.9× (all solvers).

Four-mode single-deck verification (`frequency-sweep-dipole`, Hallen, 3 repeats):

| Mode | Avg elapsed (ms) |
|:-----|----------------:|
| CPU single-thread (`RAYON_NUM_THREADS=1`, `--exec cpu`) | 13.9 |
| CPU multithread (`RAYON_NUM_THREADS=$(nproc)`, `--exec hybrid`) | 16.5 |
| GPU stub (`--exec gpu`) | 14.1 |
| Hybrid (`--exec hybrid`, GPU stub enabled) | 16.8 |

**Locked**: 2026-04-27.  To update the baseline, run the benchmark suite,
replace the CSV references, and re-run `pi-benchmark-compare.sh` to confirm
no mode drift.

---

## Criterion 2 — Solver numerical tolerance validated on 4+ corpus decks

**Rationale**: GPU kernels that produce wrong numbers in subtle ways must be
caught immediately.  The tolerance gates must already be passing on all
in-scope corpus decks before GPU paths diverge the numerical results.

### Tolerance matrix (from [`docs/requirements.md`](requirements.md))

| Metric | Tolerance |
|:-------|:----------|
| Input resistance R (Ω) | ≤ 0.1 % relative, or ≤ 0.05 Ω absolute (whichever is wider) |
| Input reactance X (Ω) | ≤ 0.1 % relative, or ≤ 0.05 Ω absolute (whichever is wider) |

Exceeding any tolerance on any corpus case is a CI failure (not a warning).

### Required state

At least **4 distinct corpus decks** must pass the tolerance gate in CI at
the time Phase 5 begins.  Each passing deck must appear in
`corpus/reference-results.json` with an accepted reference value.

### Current state — **MET**

The following corpus decks pass the CI tolerance gate (validated through
`apps/nec-cli/tests/corpus_validation.rs`):

| # | Corpus deck | Key result (Z_re + jZ_im Ω) | Reference source |
|:-:|:------------|:---------------------------|:-----------------|
| 1 | `dipole-freesp-51seg.nec` | 74.24 + j13.90 | Python MoM / xnec2c |
| 2 | `dipole-ground-51seg.nec` | 35.09 + j19.81 | xnec2c (image method) |
| 3 | `dipole-loaded.nec` | 12.4 − j918 | NEC-2 reference |
| 4 | `yagi-5elm-51seg.nec` | driven element impedance within tolerance | xnec2c |
| 5 | `frequency-sweep-dipole.nec` | all 5 sweep points within tolerance | xnec2c |
| 6 | `multi-source.nec` | both source records within tolerance | xnec2c |

**6 corpus decks** currently pass CI tolerance gates — well above the 4-deck
minimum.

---

## Criterion 3 — Phase 4 plugin surface declared stable

**Rationale**: Phase 5 GPU work must not silently break the extension-point
contracts that Phase 4 shipped.  All four extension points (EP-1 through EP-4)
must be exercised by tests, documented in `docs/plugin-api-design.md`, and
free of known breaking-change issues before GPU paths are added.

### Required state

1. All four EP traits are present in their respective crates with passing
   doctests and unit/integration tests.

2. `docs/plugin-api-design.md` contains a section for each EP with its trait
   signature, stage, and usage guidance.

3. No open issues track a planned breaking change to any EP trait signature.

4. `cargo test` passes cleanly across all crates with no EP-related failures.

### Extension point inventory

| ID | Trait | Crate | Stage | Tests |
|:---|:------|:------|:------|:------|
| EP-1 | `DeckPostProcessor` | `nec_model` | After parse, before geometry build | doctest + unit tests |
| EP-2 | `ResultFilter` | `nec_report` | After solve, before report render | doctest + unit tests |
| EP-3 | `ReportSection` | `nec_report` | Appended after standard report sections | doctests + 4 unit tests + `render_text_report_with_sections` integration |
| EP-4 | `DeckValidator` | `nec_model` | After EP-1, before geometry build | doctest + 7 unit tests + 4 CLI integration tests |

### Current state — **MET**

All four EPs are implemented, documented in
[`docs/plugin-api-design.md`](plugin-api-design.md), and passing their full
test suites as of Phase 4 completion (2026-05-03).  No breaking-change issues
are open against any EP trait.

`cargo test` result at Phase 4 close: all test results ok across all crates.

---

## Criterion 4 — `cargo deny` dependency policy passes

**Rationale**: GPU work will add new dependencies (OpenCL/ROCm/Vulkan
bindings, WGPU, or similar).  The dependency policy must already be in a
passing, non-suppressed state so that new GPU deps are evaluated against a
clean baseline rather than a waived backlog.

### Required state

`cargo deny check` exits 0 with no `skip` entries added solely to unblock
Phase 5.  Any waivers required for existing deps must be documented in
`deny.toml` with a rationale comment.

### Current state — **MET**

`cargo deny check` passes as of PH4-CHK-001 (2026-05-02).  The policy is
documented in `deny.toml`; any exceptions carry inline rationale comments.

---

## Criterion 5 — Phase 4 checklist complete

**Rationale**: Phase 5 is only meaningful if the full Phase 4 scope has
shipped.  Partial Phase 4 work left open would create competing priorities
during the GPU milestone.

### Required state

All seven PH4-CHK items must be in state `Done` in `docs/roadmap.md`.

### Current state — **MET**

| Item | Title | Status |
|:-----|:------|:------:|
| PH4-CHK-001 | `cargo deny` dependency policy | Done |
| PH4-CHK-002 | EP-3 `ReportSection` trait | Done |
| PH4-CHK-003 | `--output-format json` | Done |
| PH4-CHK-004 | Python bindings (`fnec_py`) | Done |
| PH4-CHK-005 | EP-4 `DeckValidator` trait | Done |
| PH4-CHK-006 | Automation guide | Done |
| PH4-CHK-007 | Phase 5 entry criteria (this document) | Done |

---

## Go / No-Go summary

| # | Criterion | Status |
|:-:|:----------|:------:|
| 1 | CPU baseline benchmarks locked (2+ targets, 81-row matrix) | ✅ Met |
| 2 | Solver tolerance validated on 4+ corpus decks | ✅ Met (6 decks) |
| 3 | Phase 4 plugin surface declared stable (EP-1…EP-4) | ✅ Met |
| 4 | `cargo deny` dependency policy passes | ✅ Met |
| 5 | Phase 4 checklist complete | ✅ Met |

**All five criteria are met as of 2026-05-03.**

Phase 5 GPU acceleration work may begin.  The first Phase 5 milestone is GPU
acceleration of the postprocessing (pattern interpolation and report
generation) path; see the Phase 5 deliverables list in
[`docs/roadmap.md`](roadmap.md) for the full milestone sequence.

---

## Re-evaluation

This document is a **gate**, not a snapshot.  If Phase 5 work is paused and
resumed after a significant gap, re-check all five criteria against the state
at that time before proceeding.  Update the "Current state" sections and the
`last_updated` frontmatter field when re-evaluating.

---
project: fnec-rust
doc: docs/plugin-api-design.md
status: living
last_updated: 2026-04-30
---

# Plugin API Design

## Overview

fnec-rust exposes a bounded extension surface that lets callers inject
behaviour at two well-defined points in the solve pipeline without touching
solver internals or breaking determinism guarantees.  This document describes
the extension surface, the safety model that governs it, and the first two
working extension points (EP-1 and EP-2).

This document also records the resolution of blocker **BLK-004** (Plugin API
design, safety model, first two extension points working).

## Design goals

1. **Safe by construction.** Extension implementations are plain Rust
   closures or structs passed in by the caller.  No dynamic loading, no FFI,
   no network or filesystem capability is granted through the extension
   interface itself.
2. **Solve-pipeline isolation.** Extensions run at stage boundaries (after
   parse / after solve) so a buggy extension cannot corrupt mid-solve state.
3. **Embeddability.** The traits live in stable core crates (`nec_model`,
   `nec_report`) so downstream library users can implement them without
   depending on the CLI binary.
4. **Incremental.** Only the minimum surface needed to satisfy GAP-004 is
   stabilised here.  Future extension points (deck validators, pattern
   post-processors, custom report sections) will follow the same pattern.

## Safety model

| Property | Mechanism |
|:---------|:----------|
| No network access | Trait arguments are pure Rust value types; no socket or async runtime is passed in |
| No filesystem access | Trait arguments carry no path, file descriptor, or I/O handle |
| No FFI | Trait implementations are `dyn Trait` — `unsafe` is not required or expected |
| Solver determinism preserved | EP-1 runs after full parse; EP-2 runs after solve — neither intercepts the numerical assembly |
| Caller controls trust boundary | Extension objects are created and owned by the caller; the library never constructs them |

## Extension points

### EP-1 — `DeckPostProcessor` (`nec_model` crate)

**Stage**: immediately after deck parsing, before geometry resolution.

**Signature** (in `crates/nec_model/src/lib.rs`):

```rust
pub trait DeckPostProcessor {
    fn process(&mut self, deck: &mut deck::NecDeck);
}
```

**Intended uses**:

- Tag renaming / renumbering before geometry build.
- Card injection (e.g. inserting a default `GN` card when none is present).
- Custom validation annotations that produce structured diagnostics.
- Deck normalisation for downstream comparison or hashing.

**Exercised by**: the doctests in `crates/nec_model/src/lib.rs`
(`CountCards` example — verifies the trait is callable and the process
method receives the deck).

### EP-2 — `ResultFilter` (`nec_report` crate)

**Stage**: after solve, before report rendering.

**Signature** (in `crates/nec_report/src/lib.rs`):

```rust
pub trait ResultFilter {
    fn filter(&self, rows: &[FeedpointRow]) -> Vec<FeedpointRow>;
}
```

**Intended uses**:

- Dropping feedpoint rows above an impedance threshold (e.g. for sweep
  post-processing where only matched points are of interest).
- Re-ordering or deduplicating rows before rendering.
- Injecting synthetic reference rows for comparison report formats.
- Feeding filtered results to a custom report section without going through
  `render_text_report`.

**Exercised by**: the doctests in `crates/nec_report/src/lib.rs`
(`DropHighImpedance` example — verifies the trait is callable and the filter
correctly passes or drops rows based on `z_in.re`).

### EP-3 — `ReportSection` (`nec_report` crate)

**Stage**: after all standard report sections have been rendered.

**Signature** (in `crates/nec_report/src/lib.rs`):

```rust
pub trait ReportSection {
    fn render(&self) -> String;
}

pub fn render_text_report_with_sections(
    input: &ReportInput<'_>,
    extra_sections: &[&dyn ReportSection],
) -> String;
```

**Intended uses**:

- Appending a summary statistics block (e.g. peak |Z|, SWR, resonant frequency)
  for use in sweep post-processing output.
- Injecting a custom legend or metadata block into a saved report file.
- Attaching per-run notes, git-revision stamps, or operator comments.
- Composing multi-section reports that combine standard and custom output
  without forking the rendering pipeline.

**Exercised by**: the doctests in `crates/nec_report/src/lib.rs`
(`ImpedanceSummary` and `Banner` examples — verify `render()` is called
and its output appears after the standard sections).  Four additional unit
tests in the `tests` module cover: no-extra-sections identity, single
section append, multiple-section ordering, and a `PeakImpedanceSection`
worked example.

### EP-4 — `DeckValidator` (`nec_model` crate)

**Stage**: after parsing and EP-1 post-processing, before geometry build.

**Signature** (in `crates/nec_model/src/lib.rs`):

```rust
pub trait DeckValidator {
    fn validate(&self, deck: &NecDeck) -> Vec<ValidationDiagnostic>;
}

pub struct ValidationDiagnostic {
    pub message: String,
    pub level: DiagnosticLevel,  // Error | Warning
}

pub fn run_validators(
    deck: &NecDeck,
    validators: &[&dyn DeckValidator],
) -> Vec<ValidationDiagnostic>;
```

`run_validators` runs all validators in order and aggregates their diagnostics;
it never short-circuits, so callers receive the complete picture in one pass.
`DiagnosticLevel::Error` signals that the solve should be aborted.
`DiagnosticLevel::Warning` is informational.

**Intended uses**:

- Verifying that mandatory cards are present (e.g. at least one EX card).
- Enforcing project-specific geometry conventions (wire tag ranges, segment
  count limits, frequency band restrictions).
- Checking that templated-variable substitutions produced plausible values
  before committing to a solve.
- Emitting structured feedback to automation pipelines without parsing CLI
  stderr heuristically.

**CLI integration**: `fnec` runs `NoExCardValidator` (warning-level) as a
built-in validator on every solve path, emitting `warning: [validator] …`
to stderr.  Error-level diagnostics cause a non-zero exit code without
starting the solver.

**Exercised by**: the doctest in `crates/nec_model/src/lib.rs`
(`RequireExCard` example) plus 7 unit tests in the `tests` module.  Four
integration tests in `apps/nec-cli/tests/deck_validator.rs` verify the
CLI warning/error emission path.

```
NEC deck file
     │
     ▼
 nec_parser::parse()
     │
     ▼
 NecDeck  ◄────── EP-1: DeckPostProcessor::process(&mut deck)
     │
     ▼
 [validators]  ◄── EP-4: DeckValidator::validate(&deck)
     │
     ▼
 geometry build + solver
     │
     ▼
 Vec<FeedpointRow>  ◄──── EP-2: ResultFilter::filter(&rows)
     │
     ▼
 render_text_report_with_sections()  ◄── EP-3: ReportSection::render()
     │
     ▼
 stdout / report file
```

## Planned future extension points

The following extension points are scoped for later phases but are not yet
stabilised.  They are listed here so the overall design intention is
visible:

| ID | Stage | Crate | Purpose |
|:---|:------|:------|:--------|
| EP-5 | After pattern computation | `nec_report` | `PatternFilter` — post-process `PatternRow` slice |
| EP-6 | After geometry build | `nec_solver` | `SegmentTransform` — modify the `Segment` list |

## BLK-004 resolved signal

**BLK-004** is resolved as of 2026-04-30:

- Plugin API design document exists (`docs/plugin-api-design.md`).
- Safety model is documented and enforced by construction.
- Two extension points (EP-1 `DeckPostProcessor`, EP-2 `ResultFilter`) are
  implemented in `nec_model` and `nec_report` respectively.
- Both are exercised by doctests that compile and pass under `cargo test`.
- The gate condition for Phase 3 → Phase 4 progression is met.

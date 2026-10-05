---
project: fnec-rust
doc: docs/dev/parity-sweep-design.md
status: living
last_updated: 2026-10-05
---

# Parity sweep — design (draft for review)

## Why

The FND-197/198 audit (`docs/dev/reviews/review-261004.md`) found eight of thirteen
defects in one class: **a decision keyed on one axis while the run has another** — a guard
reading the deck's `LD` cards when the load came from `--loads-config`, the GPU pattern arm
keyed on `--exec` alone while the solve keyed on the ground, MPIE pricing every `EX` card
but driving one, `sinusoidal` skipping the Hallén route. Each was fixed with its own test.
None of those tests would catch the *next* member of the class, because each pins one cell
of a product nobody enumerates. `loose-ends-audit` now bans closing an audit with a class
of two or more findings and no standing check; this is that check.

## Review (Opus, 2026-10-05) — what changed in this design

The first draft would have caught 5 of the 11 findings outright. It missed most of the
class members themselves (FND-204/207/209/210 — guards keyed on how an input is *spelled*),
its "Declined = any warning" outcome asserted nothing (every `--exec gpu` run already prints
an `info:` line), R-ground could not fail on a misapplied correction (FND-206/208), R-loss
used quantities fnec does not print, and on a host without a GPU R-exec compared the CPU
with the CPU. The enumeration also missed the axes that mattered — input source, route,
output kind are not CLI enums. This version is built around those points.

## What it is

A CLI-level sweep (`apps/nec-cli/tests/parity_sweep.rs`, always on `CARGO_BIN_EXE_fnec`)
that runs **pairs of runs that must agree** over a product of axes and compares their
**whole outcome**: exit code, refusal reason, decline IDs, and every printed number. Its
centre is **spelling equivalence** — two spellings of one input must give one outcome:

| spelling pair | class members it attacks |
|---|---|
| `LD` card ↔ the same load in `--loads-config` (valid **and** invalid loads) | FND-197, 204, 207, 209 |
| deck `FR` ↔ the same list in `--sweep-config`; a sweep point ↔ the same single point | FND-210 |
| local run ↔ `fnec worker --stdio` (the frontend axis) | FND-199 |
| `EX 4` with i0 = the `EX 0` feed current ↔ `EX 0` | FND-198 |
| `--exec gpu` / `hybrid` / auto ↔ `--exec cpu` | FND-205 |

Around it, relations that hold by physics and use only printed values:

| relation | must hold | catches |
|---|---|---|
| R-image | a deck touching PEC ↔ its free-space double **derived from the physics** (mirrored wires, reversed horizontal current), every output above the plane, 1e-6 | FND-201 |
| R-loss | load at the feed: gain_loaded − gain_unloaded = 10·log10(1 − R_L / Re Z_in) from the printed Z, 0.05 dB | FND-200 |
| R-network | a pure shunt `NT` at the feed: Z_in = Z_ant ∥ Z_shunt exactly | FND-208 |
| R-remedy | rerunning with a remedy a caveat names must not be refused | FND-209 |
| R-solver | `mpie`/`sinusoidal` ↔ `hallen`, per-(deck, solver) tolerance measured at two meshes and pinned | FND-202, 203 |

and committed nec2c goldens where no identity exists (GN 2 near field; the Sommerfeld
correction; captured once, provenance in the file — pc1 has no nec2c): FND-201 over finite
ground, FND-206.

## How it cannot pass while the class persists

- **Decisions are recorded, not inferred from prose.** Each run's `diag:` line gains a
  decision record: route, drive, ground model, load source, solver, and the executor **per
  stage** (solve, pattern, near field). Declined/Refused are classified by decision IDs,
  never by "any warning".
- **A pinned manifest** of expected refusals and declines, and a pinned **cell count per
  relation**: refusing everything, or an applicability predicate quietly shrinking, is a
  diff someone reviews.
- **Device evidence:** on the GPU host the sweep asserts a nonzero count of stages that ran
  on the device; on CI it says that R-exec ran CPU-vs-CPU rather than passing silently.
- **Asymmetric decks:** each geometry class also as a tilted, off-centre-fed wire (the
  near-field sign sabotage passed on vertical decks only).
- **Coverage from execution:** the sweep fails if a route, drive, executor or load-source
  value is never reached in a `Holds` cell — read from the decision records, so a new
  variant shows up unreached instead of uncovered.
- Enumeration of the CLI enums by a declarative macro that generates each enum and its
  `ALL` together (the existing `SolverMode::ALL` check passes a sixth variant inserted
  mid-list), checked by a unit test inside the binary crate — no hidden flag.

## Stages

1. **Decision record + spelling core.** The `diag:` decision fields; the sweep harness
   (pairs, whole-outcome comparison, manifest, cell counts, per-run timeout with
   capture-then-kill, bounded parallelism); spelling pairs for loads, FR/sweep, EX 4/EX 0,
   exec; R-image, R-loss. Deck set with the asymmetric variants.
2. **Frontends and networks:** worker `--stdio` as a frontend; invalid-load twins; R-network;
   R-remedy; sweep point ↔ single point.
3. **References and enumeration:** nec2c goldens (GN 2 near field, Sommerfeld); R-solver
   tolerances; the enum macro; execution-derived coverage; the sweep required in pc1's gate
   with the device-ran assertion.

## Budget

Measured: 0.04 s per 21-segment CPU run, 0.42 s per GPU run (device start-up). The raw
product is ~3000 cells; the cell list is generated and its count printed before anything is
promised. GPU cells are a minority by design (exec pairs on a few decks); parallelism bounded;
per-run timeout with capture-then-kill. At pc1's measured 0.33 % Xid rate a sweep with ~100
GPU runs meets a fault about a quarter of the time — the fallback keeps the relation true,
and the device-ran count says so.

## Stage 1 — built (2026-10-05)

`apps/nec-cli/tests/parity_sweep.rs`, 336 cells, about 100 s on pc1 (the MPIE over ground
is most of it: 7–10 s per run in a debug build). Relations: `S-load` (138), `S-exec` (69),
`S-fr` (69), `S-drive` (40), `R-loss` (18), `R-image` (2). The manifest
(`parity_manifest.txt`) pins 185 + 54 `Holds`, the refusals by text, the count per
relation and the decision values reached in `Holds` cells; on a host with a GPU the sweep
also asserts that some stage ran on the device (11 stages on pc1).

Its first run found **FND-216** — the device pattern's axial ratio was an amplitude
ratio — on the tilted deck the review asked for. Sabotage-verified by re-introducing six
past findings, one at a time, each caught by the relation meant for it:

| re-introduced | caught by |
|---|---|
| FND-205 GPU pattern over ground | `S-exec`, 16 cells |
| FND-197 a loads-config load dropped on a plane-wave deck | `S-load`, 23 cells |
| FND-201 near field without the PEC image | `R-image`, 2 cells |
| FND-200 gain correction over finite ground only | `R-loss`, 12 cells |
| FND-210 `--sweep-config` ignored | `S-fr`, 54 cells |
| FND-216 GPU axial ratio as an amplitude ratio | `S-exec`, 1 cell (the tilted deck) |

Known gaps, by design until stage 3: FND-202 (MPIE multi-source) and FND-203 (sinusoidal
on bent decks) are wrong *identically* in both spellings, so no spelling pair sees them;
R-solver and nec2c goldens are what catch them.

## Stage 2 — built (2026-10-05)

533 cells, about 110 s on pc1. New relations:

| relation | the two runs | cells |
|---|---|---|
| `S-worker` | the local CLI ↔ `fnec worker --stdio` (what a `--hosts` controller sends), bare and `LD`-loaded, `exec` cpu and gpu | 92 |
| `S-load-invalid` | a load naming no segment, and a range written backwards, as an `LD` card ↔ in `--loads-config`: refused both ways, one class | 14 |
| `R-network` | a pure shunt `NT` at the feed ↔ the antenna alone: Z_in = Z_ant ∥ Z_shunt exactly | 18 |
| `R-remedy` | a run ↔ the same run with each remedy its caveats name: not refused | 4 |
| `S-point` | one point of a 3-point sweep ↔ that frequency alone (every output and the decision record) | 53 |

The MPIE over ground is compared by `S-fr` only (7–10 s a run in a debug build); `S-point`
keeps the MPIE in free space.

Sabotage-verified, each caught by its relation, with a clean sweep after:

| re-introduced | caught by |
|---|---|
| the worker dropping `LD` loads | `S-worker`, 46 cells |
| FND-204: a `--loads-config` load naming no segment accepted | `S-load-invalid`, 42 cells |
| FND-209: the remedy chosen from the deck's cards (two edits) | `R-remedy`, 1 cell |
| an `NT` Y11 stamped with the wrong sign | `R-network`, 18 cells |
| every sweep point solved at the first frequency | `S-point`, 40 cells |

`R-remedy` can test only remedies a caveat names. The low-ground caveat names none,
although `--ground-solver sommerfeld` exists for a single straight wire — FND-217.

---
project: fnec-rust
doc: docs/sommerfeld-level2-scope.md
status: living
last_updated: 2026-08-22
---

# Sommerfeld–Norton "Level 2" — scoping the real remaining gap (BL-IMPR-015)

**Outcome (2026-08-22): the backlog item overstates the gap.** A code + empirical
review found that fnec already delivers most of what BL-IMPR-015 asks for. This
note records the *actual* current state, narrows the remaining work, and — unlike
the Leeson study — confirms the remaining work is **implementable and validatable**
(there is a real oracle). The detailed physics/algorithm design already exists in
[`ph9-chk-006-sommerfeld-ground.md`](ph9-chk-006-sommerfeld-ground.md); this is the
scoping layer on top of it.

## What fnec already does (verified)

- **`--ground-solver sommerfeld` (Hallén path, "Level 1")** — corrects the
  near-ground **feedpoint Z** of any *straight* wire (arbitrary orientation) with
  the Sommerfeld/Norton surface wave, via a post-solve induced-EMF reaction
  integral (`sommerfeld::ground_z_correction`). Declines bent geometry (keeps RCM).
- **`--solver mpie` (+ Sommerfeld kernels in the Z-fill)** — already produces the
  full near-ground solution — **per-segment currents AND radiation pattern** — for
  **straight and bent** wires entirely above ground. It fills the reflected
  potential kernels (`sommerfeld::reflected_potential_kernels`) on a ρ-grid and
  adds them to the free-space Z, so ground fill is ~free-space cost + O(grid)
  (`mpie.rs`).

Verified 2026-08-22: `fnec --solver mpie --ground-solver sommerfeld` on a low
horizontal dipole over GN2 (`GN 2 … 13.0 0.005`, height 0.14 λ) emits 22 current
rows and a `RADIATION_PATTERN` table plus a feedpoint Z. So "near-ground currents
and patterns on arbitrary geometry above ground" — the headline of BL-IMPR-015 —
**already works** through the MPIE path.

## The genuine remaining gap (narrowed)

1. **DCIM for speed/generality.** The MPIE reflected fill uses the *exact* kernel
   on a ρ-grid. The Discrete Complex Image Method (GPOF/Prony fit → a short sum of
   complex images) would replace the grid with a handful of closed-form image
   terms — faster and cleaner, and the same mechanism could lift the Hallén
   `--ground-solver sommerfeld` path from feedpoint-Z to full currents. The GPOF
   design and the azimuthal `J0/J1/J2` reduction are already written up in
   `ph9-chk-006` (§ "Recommended implementation path — DCIM").
2. **Hallén-path parity.** Bring `--ground-solver sommerfeld` (the Hallén path) to
   the currents/patterns + bent-geometry coverage MPIE already has, or document
   MPIE as the supported near-ground-currents route and keep the Hallén path as
   the fast feedpoint-Z correction.
3. **Sub-0.1 λ accuracy.** Tighten and gate the very-low-height regime where the
   surface wave dominates (the low-height sign flip is already reproduced for the
   straight horizontal case to ~13 %).

## Why this is *unlike* the Leeson item

- **There is an oracle.** `nec2c` GN2 is the reference, and fnec's MPIE already
  matches it to < 8 % for straight wires and ~9 % for an inverted-V over GN2. Any
  Level-2 change can be gated against nec2c GN2 — no measurement/book dependency.
- **The physics is validated** (Level 1 + MPIE landed and gated) and the algorithm
  is designed (`ph9-chk-006`), with Python prototypes under
  `studies/sommerfeld-ground/`.

So the risk here is *effort*, not *correctness-without-a-reference* (the Leeson
blocker). This is a phased numerical implementation, not a research dead-end.

## Recommendation

1. **Re-scope BL-IMPR-015** away from "add near-ground currents/patterns" (done via
   MPIE) toward the three items above — primarily **DCIM** as the speed/generality
   upgrade and Hallén-path unification.
2. Implement DCIM in phases against the `ph9-chk-006` design and the
   `studies/sommerfeld-ground/` prototypes, gating each phase against nec2c GN2
   (feedpoint Z, then current distribution, then elevation pattern) with stated
   tolerances.
3. Treat it as several PRs (GPOF fit → image extraction → Z-fill integration →
   pattern), not one — comparable in size to the original PH9-CHK-006/007 arc.

## References

- `docs/ph9-chk-006-sommerfeld-ground.md` — physics derivation + DCIM design.
- `docs/mpie-solver-scope.md` — the MPIE near-ground currents/patterns scope.
- `studies/sommerfeld-ground/` — Python prototypes (`efie_mpie_ground.py`, etc.).
- `crates/nec_solver/src/sommerfeld.rs`, `crates/nec_solver/src/mpie.rs`.

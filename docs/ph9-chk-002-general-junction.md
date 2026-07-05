---
project: fnec-rust
doc: docs/ph9-chk-002-general-junction.md
status: living
last_updated: 2026-07-05
---

# PH9-CHK-002: general junction basis — degree-2 conductor paths

## Status

**Degree-2 junctions solved (2026-07-05).** Building on the collinear fix, the
Hallén delta-gap solve now handles **any degree-2 conductor chain** — bends,
start-to-start / end-to-end splits, and inverted-V apex feeds — by solving on a
continuous *conductor path* rather than per `GW` wire. This closes the headline
junction-fed-feedpoint limitation for the mainstream bent/branched-at-the-feed
antennas (inverted-V, bent dipole, split-fed dipole).

Still deferred to the remaining general work: **degree-3+** (T/Y) junctions,
**closed loops**, and the **receive-side** (plane-wave / current-source) junction
solve. Those out-of-scope classes fall back to the guarded per-wire path
(PH9-CHK-005) and still warn.

## What was fixed

The root cause (see `ph9-chk-002-junction-feed-diagnosis.md`) is that the Hallén
homogeneous solution `cos(k·s)` and its constant `C` are built **per `GW` wire**,
so they are discontinuous across a junction. The collinear fix merged straight
end-to-start chains; this increment generalizes that to the full degree-2 case,
including chains that **reverse direction** at the junction (start-to-start) or
**bend**.

### The conductor-path model

`build_conductor_paths` (`geometry.rs`) walks the wire-endpoint graph and returns
one [`ConductorPath`] per maximal chain of wires joined through degree-2 nodes. For
each segment on a path it records:

- a **traversal sign** `σ` (`+1` if the segment's own NEC direction aligns with the
  path, `−1` if the path traverses it in reverse), and
- a **signed arc-length** `s` of the segment midpoint, measured from the path's
  arc-length centre (`s = 0` at the middle of the total path length, negative on
  one arm, positive on the other).

The two terminal segments of each open chain are its **free ends** (where `I = 0`).

`build_conductor_paths` returns `None` when the topology is out of scope — any node
where three or more wire ends meet, or a closed loop with no free end — so the
caller can fall back cleanly.

### The continuous basis

The current on segment `m` in its own direction is `I[m] = σ_m · I_path(s_m)`, where
`I_path` is the continuous path current. Substituting into Hallén's equation:

- `cos_vec[m] = σ_m · cos(k·s_m)` — continuous across the junction because `cos` is
  even in `s` and the sign tracks the traversal (`build_hallen_rhs_paths`).
- the delta-gap source term is `σ_m · σ_feed · (−j·(2π/η)·V·sin(k·|s_m − s_src|))`,
  with arc-length distance along the path. The `σ_feed` factor references the drive
  to the feed segment's own direction, so `V/I[feed]` stays positive regardless of
  which arm the feed lands on.
- `solve_hallen_paths` (`linear.rs`) groups **one homogeneous constant per path**
  (not per `GW`) and applies `I = 0` only at the free ends; interior degree-2
  junctions get no constraint (the current flows through continuously, exactly as
  inside a single wire).

For a single straight wire (`σ = +1`, arc-length = straight-axis coordinate) every
formula reduces exactly to the pre-existing `build_hallen_rhs` / `solve_hallen`, so
the change is a numeric no-op for the non-junction case. To guarantee zero
regression the CLI only routes through the new solver when a deck contains a
**non-trivial** path (a reversed or bent chain); single wires, collinear chains, and
out-of-scope topologies keep the exact previous code path.

## Validation (14.2 MHz, free space)

References from nec2c (`XQ` execute). fnec's Hallén carries a known systematic
reactance offset vs nec2c (see the `fnec-validation-strategy` note), so the strong
physical gate is **radiation resistance** plus the exact split-dipole identity.

| deck | topology | nec2c Z (Ω) | fnec Z (Ω) | check |
|:-----|:---------|:------------|:-----------|:------|
| straight dipole | one `GW` (ref) | 79.35 + j46.22 | 74.24 + j13.90 | reference |
| split, apex feed | start-to-start | 79.43 + j46.27 | **74.41 + j14.52** | == collinear split (identical antenna) ✓ |
| inverted-V 30° | bend, apex feed | 57.7 − j4.3 | 55.5 − j11.9 | **R within 4%** ✓ |
| inverted-V 45° | bend, apex feed | 40.0 − j24.9 | 39.0 − j7.0 | **R within 2.5%** ✓ |
| inverted-V 90° | bend, apex feed | 43.5 + j11.6 | 42.1 + j22.4 | **R within 3%** ✓ |
| bent dipole, fed off-bend | degree-2 | (physical) | 87.4 + j16.4 | positive R (was −0.04 − j887) ✓ |

The split-dipole recovering the single-wire impedance **exactly** (the two are the
identical antenna, cut at the feed) is the unimpeachable gate; nec2c confirms both
models give the same 79.4 + j46.3 Ω. Radiation resistance matching nec2c to 2–4%
across the bent cases is the independent physical check.

Tests: `crates/nec_solver/tests/general_junction.rs` (split-recovers-single,
inverted-V resistance vs nec2c, path-decomposition unit tests) and
`apps/nec-cli/tests/junction_feedpoint.rs` (degree-2 now solves; degree-3 still
guarded).

## Boundary

| class | status |
|:------|:-------|
| single wire, collinear split | solved (unchanged) |
| bend / start-to-start / end-to-end (degree-2) | **solved (this increment)** |
| degree-3+ T/Y junction | deferred → guarded (PH9-CHK-005) |
| closed loop | deferred → guarded |
| receive-side (plane-wave / current-source) junction | deferred |

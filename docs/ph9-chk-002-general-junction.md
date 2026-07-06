---
project: fnec-rust
doc: docs/ph9-chk-002-general-junction.md
status: living
last_updated: 2026-07-06
---

# PH9-CHK-002: general junction basis — degree-2 conductor paths

## Status

**Degree-2 junctions solved (2026-07-05).** Building on the collinear fix, the
Hallén delta-gap solve now handles **any degree-2 conductor chain** — bends,
start-to-start / end-to-end splits, and inverted-V apex feeds — by solving on a
continuous *conductor path* rather than per `GW` wire. This closes the headline
junction-fed-feedpoint limitation for the mainstream bent/branched-at-the-feed
antennas (inverted-V, bent dipole, split-fed dipole).

**Receive-side (plane-wave) junctions solved end-to-end (2026-07-05).** The same
conductor-path model now backs a *distributed*-excitation solver
(`solve_hallen_planewave_paths` / `build_planewave_hallen_paths`), and the CLI
receive path (`solve_plane_wave_hallen` in `solve_session.rs`) routes junctioned
degree-2 geometry through it — so a **receiving** bent or connected antenna solves
and emits a `RECEIVE_PATTERN` where it previously failed fast. See
[Receive-side junctions](#receive-side-junctions-plane-wave) below.

Still deferred to the remaining general work: **degree-3+** (T/Y) junctions,
**closed loops**, and the **current-source** receive-side junction solve. Those
out-of-scope classes fall back to the guarded per-wire path (PH9-CHK-005) and still
warn.

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

## Receive-side junctions (plane wave)

The transmit fix above solves a *symmetric* delta-gap source, for which one
homogeneous constant (`cos(k·s)`) per path suffices. A **receiving** antenna sees a
*distributed, asymmetric* incident field, so its Hallén homogeneous solution needs
**both** degrees of freedom — `C_cos·cos(k·s) + C_sin·sin(k·s)` in the path
arc-length `s`. `solve_hallen_planewave_paths` (`linear.rs`) is the path-aware
counterpart of `solve_hallen_planewave`: two `C` columns per conductor path, and the
`I = 0` boundary condition applied at each path's **two free ends only**, so the
induced current stays continuous across the junction.

`build_planewave_hallen_paths` (`planewave.rs`) builds the forcing over the whole
conductor path with the same sign + signed-arc-length convention as
`build_hallen_rhs_paths`: the incident tangential field is taken in the path
traversal direction (`E_path(s_p) = σ_p·(ê·d̂_p)·E₀·exp(+j k r̂·r_p)`), the
`sin(k|s_m−s_p|)` kernel sums over the entire path (not resetting per `GW`), and
`cos_vec[m] = σ_m·cos(k·s_m)`, `sin_vec[m] = σ_m·sin(k·s_m)`. For a single straight
wire this reduces exactly to `build_planewave_hallen`.

### Validation (14.2 MHz, free space)

Two internal gates, neither needing an external reference
(`crates/nec_solver/tests/planewave_junction.rs`):

| gate | geometry | result |
|:-----|:---------|:-------|
| **degeneracy** | λ/2 dipole as a start-to-start split (one arm reversed), identical 52-seg mesh | path receive solver reproduces the validated per-wire solver's peak current to **~1e-11** (machine precision), θ = 35–90° |
| **reciprocity** | bent inverted-V (not collinear), apex feed | short-circuit feed current tracks the transmit far-field: `|I_feed|²/G_θ` constant to **1.5 %** across θ = 40–85° (≈8× gain range) |

The degeneracy gate proves the sign / arc-length bookkeeping is exactly right on a
reversed arm; the reciprocity gate proves the genuinely-bent case is physically
correct against the already-validated conductor-path transmit + farfield paths.

End-to-end CLI gate (`apps/nec-cli/tests/receive_junction.rs`): a start-to-start
split dipole illuminated by a `NTHETA` incidence sweep now solves through the CLI
and emits a `RECEIVE_PATTERN` with the correct z-dipole shape (endfire null,
broadside peak, monotonic between), and its normalized receive pattern matches its
own normalized transmit gain pattern by reciprocity to **0.025 dB** — both sides
solved on conductor paths.

## Current-source junctions (EX type 4)

The EX-type-4 current source is the **symmetric-source** cousin of the plane-wave
receive path: it forces a known current `i0` at the feed and solves for the port
voltage `V` (feedpoint `Z = V/i0`). Because the driven current is symmetric about
the feed — exactly like the voltage delta-gap — it needs only **one** homogeneous
constant `cos(k·s)` per conductor path (not the plane-wave's two), plus the single
unknown `V`.

`solve_hallen_current_source_paths` (`linear.rs`) is the path-aware counterpart of
`solve_hallen_current_source`: one `C` column per path, the port-voltage column, the
`I = 0` constraint at each path's two free ends, and the forced `I[src] = i0` row
(the exact rows heavily weighted so `V/i0` recovers the true impedance).
`build_current_source_shape_paths` (`excitation.rs`) builds the unit-voltage source
shape `g` over the path via `build_hallen_rhs_paths`, so it stays continuous across
the junction. For a single straight wire both reduce to the pre-existing per-wire
functions.

### Validation (14.2 MHz, free space)

Internal consistency — the port impedance is a property of the antenna, independent
of drive, so the current-source `Z = V/i0` must equal the voltage-source
`Z = 1/I_feed` on the *same* junctioned geometry
(`crates/nec_solver/tests/current_source_junction.rs`):

| geometry | voltage-source Z (Ω) | current-source Z (Ω) | rel |
|:---------|:---------------------|:---------------------|:----|
| start-to-start split dipole, apex feed | 74.41 + j14.52 | 74.40 + j14.52 | ~2×10⁻⁴ |
| bent inverted-V, apex feed | 55.53 − j11.94 | 55.51 − j11.94 | ~3×10⁻⁴ |

The forced feed current is honoured to <1e-4, and doubling `i0` leaves `Z` unchanged
(linearity).

## Boundary

| class | status |
|:------|:-------|
| single wire, collinear split | solved (unchanged) |
| bend / start-to-start / end-to-end (degree-2) — transmit (voltage delta-gap) | **solved** |
| bend / start-to-start / end-to-end (degree-2) — plane-wave receive | **solved (CLI-wired)** |
| bend / start-to-start / end-to-end (degree-2) — current source (EX type 4) | **solve core landed; CLI wiring pending** |
| degree-3+ T/Y junction | deferred → guarded (PH9-CHK-005) |
| closed loop | deferred → guarded |

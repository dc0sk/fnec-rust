---
project: fnec-rust
doc: docs/hallen-free-end-refinement.md
status: living
last_updated: 2026-10-10
---

# Free-end refinement (FND-227, FND-230)

**Status: design, reviewed (Fable, 2026-10-10) and revised. Nothing here is implemented yet.**

## The defect

Hallén's delta gap enters as `sin(k·|s − s_f|)` sampled at segment midpoints. On
the segment at a free wire end every other midpoint lies on one side of the gap, so
the source vector lies in the span of the homogeneous `cos(k·s)`, `sin(k·s)`
columns and the constants absorb it (FND-227; refused since #562). A lumped load
there is stamped with the same shape and is inert (FND-230). The cause is
structural: a pulse current on the end segment cannot fall from its own value to
zero at the tip, so nothing between the gap and the tip constrains the solve.

The same coarse end segment costs every Hallén answer accuracy. The maintainer's
51-segment reference dipole (10.564 m, 14.2 MHz, centre-fed; Ω):

| segs | fnec now | both ends split in 3 | nec2c 1.3.1 |
|---:|:--|:--|:--|
| 21 | 77.55 + j37.58 | 78.96 + j43.48 | 79.03 + j45.86 |
| 51 | 78.83 + j42.43 | 79.33 + j45.04 | 79.35 + j46.22 |
| 101 | 79.21 + j44.18 | 79.48 + j45.68 | 79.49 + j46.41 |

(Explicit split decks, `GW 1 3 / GW 2 N−2 / GW 3 3`, solved by the product today.)

**Decision (maintainer, 2026-10-10): refine every free end, not only stamped ones.**
Refining only stamped ends would make a load's apparent effect include the
refinement's own effect (21-segment λ/2 at 29.98 MHz: unloaded 78.02 + j36.53 →
split 79.55 + j42.53, a 6 Ω reactance shift before any load).

## Measured facts the design rests on

- **The split agrees with the solver's own uniform mesh.** End-fed λ/2 wire, gap held
  at d = 0.119 m from the tip: K=3 split, Hallén 1510.6 − j4102.9; uniform N=63
  (gap on segment 2, same d) 1536.7 − j4097.7 (1.7 % in R). The reviewer's case (8.33 m,
  18 MHz): split 1463.5 − j6721.7, uniform 3N with the gap on segment 2, 1457.4 − j6749.0
  (0.41 %); nec2c's own pair differs 0.6 %. MPIE K=3 2377.7 − j4847.3; MPIE
  uniform N=63, 2366.8 − j4864.9. Graded meshes are not the problem.
- **End-fed impedance is ill-conditioned for every code.** Same fixed d, uniform
  meshes N = 63 … 735: Hallén R 1537 → 1381, MPIE 2367 → 1490, nec2c 1943 → 1439;
  none converged. Linear-in-1/N extrapolation from 483 / 735 gives 1332 / 1388 / 1364
  (within 4 %), with h/r ≈ 7 at 735 (thin-wire kernel marginal). **So the gate for the
  end-fed case is self-consistency (split = own uniform mesh at the same gap) plus a
  loose nec2c bound, not tight nec2c agreement.**
- **A load on the centre sub-segment is seen.** 1000 Ω on the end of a centre-fed
  21-segment wire, split: ΔZ = +7.79 − j3.74; nec2c +8.25 − j3.10.

## Design

### Where: once, at the end of `build_geometry`

After GM/GR and before the renumbering pass (`geometry.rs:865`, which must then skip
flanks so the running per-tag count is the deck's), every segment with
a **free end** is replaced by K = 3 equal sub-segments along it. Every product path
(CLI, sweep, resonance, `--hosts` workers, GUI, fnec_py) calls `build_geometry`, so
one site covers them all — no per-pipeline opt-in that one pipeline forgets.

A **free end** is a segment endpoint that:
- coincides (within `JUNCTION_TOL_M`) with no endpoint of any other segment, and
- lies on no other segment's interior (the `split_at_touching_ends` touch rule), and
- is not on the ground plane (`z ≤ GROUND_CONTACT_EPS_M`) when the deck has a ground,
  as `ground_model_from_deck` decides it (`GE 1` without `GN` is PEC): a grounded end
  is a junction with its image.

A one-segment wire with two free ends is split once (3 sub-segments), not twice.
Decided on the final geometry, one predicate, a new `pub fn free_end_segments`.

### Identity: the deck's numbering is untouched

`Segment` gains `pub part: SegmentPart` — `Whole` (default, every unrefined
segment), `Centre`, `Flank`.

- The **centre** sub-segment keeps the parent's `tag` and `tag_index`. Its midpoint
  is the parent's midpoint. Every deck reference (`EX`, `LD`, `TL`, `NT`, `PT`, the
  first-match resolvers listed in the inventory) therefore resolves to it unchanged.
- The two **flanks** carry the parent's `tag` and `tag_index` **0** — but 0 is NOT
  unaddressable on its own: `EX 0 1 0` is a hard error today only because no segment
  is numbered 0, and with flanks it would silently match the first flank (an inner one
  when the free end is the far end, which the #562 tripwire does not catch). So **every
  `(tag, segment)` lookup goes through one resolver**, `find_deck_segment(segs, tag,
  seg)`, which requires `seg ≥ 1` and `part != Flank`, replacing the ~25 inline
  `.position(|s| s.tag == … && s.tag_index == …)` copies; a source-scanning gate bans
  the inline pattern outside it. The sites that treat 0 as a wildcard skip flanks:
  - `loads::segment_matches` (`LD … 0` = "all segments of the tag"): a **lumped**
    load (types 0–4, Laplace) lands on the centre only; type 5 (conductivity, scaled
    by length) on all three — its loss is per metre. (Its stamp on the OUTER flank is
    in the homogeneous span and inert by the FND-230 mechanism: one third of one
    segment's conductor loss is not seen. Small; documented, not hidden.)
  - `loads::laplace_segment_matches`: centre only.
  - `tl::find_center_segment_index` (port segment 0 = the tag's middle): counts
    `part != Flank` only, so the middle it picks does not move.
- **Outputs list deck segments**: the CURRENTS block, the GUI currents tab and
  fnec_py's `solve_currents_deck_str` skip flanks, through one helper
  `Segment::is_deck_segment()`. The centre's current is the current at the deck
  segment's midpoint, which is what the row has always meant. The 3D view and the
  far / near field use all segments (a finer, correct current).
- **Validation reads deck segments**: `source_risk_geometry_error`'s length/radius
  check (`< 2.0` is a hard error) uses the parent length (3 × centre), so a feed
  that passed before still passes.

### Sinusoidal basis: unchanged (FND-231, rejected by measurement)

The review suspected `solve_hallen_sinusoidal_basis` mis-samples a graded wire: its
modes are `sin(pπ·(j + ½)/n_w)`, in segment index, not arc length. Measured on the
split reference dipole, it does not — index-space modes are a legitimate discrete
basis (n_w − 1 of them on n_w segments) and keep the refinement's local resolution:

| split dipole, sinusoidal | 21 segs | 51 segs |
|:--|:--|:--|
| index-space modes (today) | 78.10 + j42.62 | 79.45 + j44.99 |
| arc-length modes | 78.30 + j39.81 | 79.03 + j43.24 |
| arc-length + length-weighted testing | 78.30 + j39.81 | 79.05 + j43.25 |
| Hallén, same split mesh | 78.96 + j43.48 | 79.33 + j45.04 |

Arc-length sampling moves it 1.8 Ω away from Hallén in X; the basis stays as it is.

### MPIE stays on the deck mesh

The MPIE is not part of this change: it is already close to nec2c on the dipole
(79.15 + j45.95) and its end-feed refusal (FND-228) stays. Coarsening at its entry is
**not** enough: the far and near field weight each segment as a point source
`I·Δl·e^{jk·r}`, so three thirds carrying the parent's current radiate differently
(~1e-3), and the parity sweep (twin runs of one binary) could not see it. So the mesh
is chosen **before** the geometry is built: `build_geometry(deck)` refines,
`build_deck_geometry(deck)` does not, and one function, `geometry_for(deck, solver)`,
picks by solver — MPIE and the experimental pulse/continuity bases get the deck
mesh; Hallén and sinusoidal the refined one (the sinusoidal basis tracks Hallén on the
refined mesh, see above). Every frontend calls `geometry_for`;
the gate is an MPIE pattern value pinned against today's.

### What changes for users

- Every Hallén and sinusoidal answer with a free end moves toward nec2c (table above).
- End-fed wires, end-segment LD, EX 4 and TL/NT ports at a free end solve; the
  #562 refusal stays as a backstop and becomes unreachable on product paths.
- Segment counts in `info:` lines and GPU eligibility count refined segments (+2 per
  free end). The `exec` crossover constants are per host and unaffected in kind.

## Gates

1. **End-fed self-consistency:** the refined end-fed answer is compared with the
   uniform 3N mesh with the gap on segment 2 (same position, same local mesh). Measured
   so far: 1.7 % (5 m, 29.98 MHz) and 0.41 % (8.33 m, 18 MHz) in R — the rest of the
   wire is meshed at h, not h/3, so the two are not the same mesh. **The bound is set
   from a measured spread over length, radius and N before the gate is written**, not
   from either case (CPU, GPU, worker alike). Negative control:
   without refinement the deck is refused (the #562 tripwire).
2. **FND-230:** end-segment LD changes Z; ΔZ within 15 % of nec2c's ΔZ.
3. **Dipole accuracy:** the reference dipole at 21 / 51 / 101 within the table's
   values ± 0.05 Ω; corpus references re-captured against nec2c, never relaxed.
4. **Identity:** every deck reference resolves to the same physical point (midpoint)
   as before — a sweep over every corpus deck comparing resolved EX/LD/TL midpoints
   between the deck mesh and the refined mesh.
5. **MPIE unchanged:** an MPIE feedpoint and pattern value pinned at today's numbers
   (the parity sweep's twin comparison cannot see a mesh change on both sides).
7. **Resolver gate:** a source scan bans inline `tag_index ==` lookups outside
   `find_deck_segment`; `EX 0 1 0` stays a hard error on a refined deck.
8. **Fat wire:** h/r = 5 end segment (flank h/r 1.7) still solves and moves toward
   nec2c (measured 90.1 + j35.0 → 95.9 + j44.4; nec2c 96.8 + j51.1); validation reads
   the parent length.
6. **Sabotages:** flanks given the parent's `tag_index`; lumped LD on all three; TL
   centre counting flanks; CURRENTS printing flanks; ground-contact end refined;
   MPIE not coarsened. Each must fail a gate.

## Fixture fallout

Every fixture whose point is a **one-segment arm with a free end** dissolves (refined
to 3, the section graph then takes it): `corpus/y-junction-negative-r-freesp.nec`
(its −5.441 Ω is the fixture for `worker_infinite_vswr.rs` and fnec_py's
`test_smoke.py`), `sinusoidal_routing.rs` `T_ONE_SEGMENT_ARM`, `sweep_contract.rs`,
`junction_feedpoint.rs`, `ground_contact.rs`, nec-gui `solve.rs`. A one-segment run
stays reachable only between two junctions; those fixtures are rewritten that way,
each still showing what it was written to show.

## Mixed-version workers

`--hosts` pools may mix versions (`protocol.rs` calls it normal; no version field). A
refined controller beside an unrefined worker would blend two meshes in one sweep,
silently. The worker reports a `mesh` field; the controller refuses a mismatch.

## Answers to the review's questions

1. A flank sentinel alone is a trap; sound only behind the one resolver (above).
2. K = 3, fixed; K must be odd for the midpoint identity; no data for a λ-based K.
3. 3D view shows the refined mesh (what was solved); Currents tab and exports list
   deck segments; `deck_write` writes cards and is unaffected.
4. Ground contact: only the fixture. GM: fine (refinement follows it). NT/TL: by
   address through the resolver; `find_center_segment_index` counts non-flanks.

## Original open questions (answered above)

1. `tag_index = 0` for flanks versus an explicit `part` check in every resolver
   (22 sites; FND-135 routed numbering through one place). Is "0 is unaddressable"
   a sound invariant, or a trap for the next resolver written?
2. K = 3 everywhere, or K chosen by the end segment's length in wavelengths?
3. Is a refined geometry in the GUI's 3D view and in exported geometry acceptable,
   or should the view show deck segments?
4. Anything in ground contact (`pec_ground_contact` copies `tag_index` to images),
   GM copies of refined wires, or `NT`/`TL` port voltage (`V = v·length`) that this
   misses?

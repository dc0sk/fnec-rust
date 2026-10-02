---
project: fnec-rust
doc: docs/hallen-bends.md
status: living
last_updated: 2026-10-02
---

# Hallén on bent conductors (FND-162)

Hallén's rows hold the **tangential** vector potential of each segment and
integrate `(∂²_s + k²) A_s = −jωμε V δ` along a straight run. A bent conductor
breaks that in two ways. The homogeneous term cannot be one function across a
corner. And at the corner the transverse divergence `∇⊥·A⊥` contributed by the
other, non-parallel section (Mei 1965's curved-wire term) is missing.

Since 2026-09-27 (stage 1b) the driven Hallén solve (delta gap and current
source) models both, and since 2026-09-28 the plane-wave receive solve too:

- **Per-section homogeneous terms.** Each straight section of a conductor path
  carries its own `(C, D)`.
- **Two rows at every bend.** Path-current continuity (both sides extrapolated to
  the node), and equal scalar potential, `∂A_s/∂s` equal on both sides.
- **The corner term.** On each section the missing part of `A_s` is
  `Q(s) = −∫_{s₀}^{s} F(s″) cos k(s − s″) ds″`, referenced at one node of the
  section. It enters the matrix; `crates/nec_solver/src/corner.rs` holds `f_n`
  and the graded quadrature. A section referenced at its other node carries its
  integral into that bend's potential row.

The formulation was worked out by the design review (Fable). It was prototyped in
numpy and gated before any Rust: with the term off, the prototype reproduced the
code's stage-1 numbers to 0.0005 %. The Rust then matched the prototype to every
printed digit.

## Measured (14.2 MHz)

| deck | nec2c 1.3.1 | before (bend rows only) | now |
|---|---|---|---|
| 90° L, 21 per arm | 60.537 − j118.240 | 84.649 − j50.199 | 59.409 − j125.053 (5.2 %) |
| 90° L, 41 per arm | 60.385 − j117.700 | 85.272 − j46.716 | 59.754 − j121.716 (3.1 %) |
| inverted-V fed ¼ up an arm, 21 | 3578.6 − j1727.0 | 2261.5 − j2484.1 | 3243.7 − j1869.9 (9.2 %) |
| inverted-V fed ¼ up an arm, 41 | 3572.5 − j959.3 | 2664.5 − j2120.9 | 3438.2 − j1088.1 (5.0 %) |
| inverted-V fed at the apex, 21 / 41 / 81 | 86.8 + j197.2 / 87.5 + j198.1 / 88.1 + j199.0 | 0.7 % / 3.0 % / 6.5 %, diverging | 4.0 % / 3.0 % / 2.5 %, converging |
| two-bend U, 11 / 21 per wire | 14.39 − j219.22 / 14.31 − j218.18 | — | 14.16 − j221.40 / 14.18 − j219.57 (≈ 1 %) |
| Z shape, 11 / 21 per wire | 45.41 − j565.43 / 44.62 − j559.29 | — | 43.30 − j567.62 / 43.42 − j560.83 (≈ 1 %) |
| steep split-V (corpus) | 268.56 + j452.26 | 344.58 + j530.33 | 270.43 + j443.20 (1.8 %) |

The apex-fed inverted-V is the case to remember. Without the corner term it read
0.7 % from nec2c at 21 segments, and the error *grew* under refinement: two errors
cancelling at one mesh. Every deck here is gated at two meshes
(`crates/nec_solver/tests/bend_corner_nec2c.rs`), with the error required to
shrink. Removing the corner term fails all three gates: the U at 88 %, the L at
57 %, the inverted-V at 40 %.

## Stage 1 on its own measured no gain

Per-section `(C, D)` plus the two bend rows, *without* the corner term, left the
L at 84.6 − j50.2 against nec2c's 60.5 − j118.2. That result refuted bend rows
without the term, not the design. The review had predicted it: the omitted term is
the same order as the jump the rows fix.

## The receive solve (2026-09-28)

The receive solve had been kept on one `(C, D)` per path, on the belief that its
forcing "jumps at a bend, which the bend rows do not carry". The forcing does not
jump. It is the incident field's tangential part convolved with `sin(k|s − s′|)`,
which is a sum of delta-gap right-hand sides, one per segment. Each of those is
smooth at every node, so the bend rows written for a feed hold for it unchanged.
A unit test in `planewave.rs` pins that identity to 1e-12. Routed through the same
section layout, bend rows and corner term, the receive currents against nec2c
(|ΔI| / peak |I|, at the middle of each run and both segments at the bend):

| deck | per path, before | now, coarse → fine mesh |
|---|---|---|
| 90° L, θ = 45°, 21 / 41 per arm | 106 % / 122 % | 6.3 % → 3.6 % |
| the same L lit edge-on (one arm sees no field) | 106 % / 121 % | 5.6 % → 3.3 % |
| two-bend U, 11 / 21 per wire | 41 % / 51 % | 1.9 % → 1.1 % |
| inverted-V, θ = 60°, 21 / 41 per arm | 8.6 % / 9.1 % | 1.7 % → 1.1 % |

A straight wire is unchanged (5.6 %, the pulse basis's own error). Sections
without the corner term gain nothing: the L stays at 121 % and the U reaches
647 %. Gates: `crates/nec_solver/tests/bend_receive_nec2c.rs`, and the absolute
reciprocity level in `planewave_junction.rs`. That level is `|I_sc|²/G =
R·λ²/(π·η₀·|Z|²)` from the transmit solve, met to 0.09 %. The per-path receive
misses it by 8 %, while the angle spread the test used to check was 0.18 % for
both.

The receive-pattern sweep builds the layout and corner term once per geometry
(`plan_hallen_planewave`). Rebuilt per direction, a 37 × 73 pattern on a 41-per-arm
L took 264 s. It now takes 10 s; the per-path solve took 6 s.

## Sources that share no node (2026-09-29, FND-162 stage 4, FND-171, FND-174)

The corner term's derivation never uses a shared node: `f_n` is the divergence
of any non-parallel source's potential, and the reference constant it needs is
absorbed by the observation section's own `(C, D)`. So it now sums over every
non-parallel source — segments on other wires, and each segment's ground image
(the geometric mirror carrying −Γ times the current, Γ the matrix's own
coefficient). Straight conductors take the path basis whenever two are not
parallel or one is not parallel to its own image, and every section carries the
term, a single straight one included. The integration also refines toward a
source endpoint that comes close to a section, not only toward the section's own
nodes.

| deck | before, coarse → fine | now | nec2c |
|---|---|---|---|
| 45° dipole 10 m over `GN 1`, 41 / 81 | 5.2 % → 7.4 % (free-space value) | 5.08 % → 2.94 % | 78.29 + j42.40 |
| apex-fed inverted-V over `GN 1`, 21 / 41 / 81 per arm | 8.4 → 11.4 → 13.0 % | 8.6 → 5.8 → 4.7 % | 54.58 + j9.27 … |
| wire 0.7 m over a fed vertical dipole, 21 / 41 | zero current | 11.0 → 6.2 % | 3.54e-4 A peak |

The design review (Fable) prototyped it in numpy, calibrated with the term off to
fnec's numbers to every printed digit, and set the kill criteria before any Rust;
the Rust reproduces the prototype to the printed digit. The inverted-V over ground
equals the same antenna with its image written out as wires in free space, to
every digit. A crossing at 5 cm stays antisymmetric to 1e-6; graded only toward
section ends it did not (centre 2.7e-4 of a 9.2e-4 peak). Gates:
`crates/nec_solver/tests/transverse_nec2c.rs`; removing the images, the routing,
the endpoint refinement or the image sign each fails them.

## Junctions and loops (FND-162 stages 2 and 3)

The same design, generalised from a path to a **section graph**
(`crates/nec_solver/src/section_graph.rs`): every straight section carries its
own `(C, D)`; a node of degree d is closed by one Kirchhoff row (the section
currents, extrapolated to the node, sum to zero) and d − 1 equal-scalar-potential
rows, each with the corner term; a closed loop is a cycle of sections and needs
no endpoint condition. The route predicate is `graph_route` in
`crates/nec_solver/src/hallen_session.rs`, and every frontend reaches it through
`solve_hallen_routed`, so the T/Y-junction, closed-loop and junction-feed
warnings no longer fire for these decks. Over `GN 2` it uses the usual
reflection-coefficient ground.

It takes a deck only when all hold: voltage (delta-gap, EX 0/5) sources, a
current source (EX 4, solved as its unit gap and scaled — stage 5), or a plane
wave (EX 1–3, a gap on every segment; stage 5, without `LD` loads); lumped `LD` loads are columns of the graph system and `TL`/`NT` networks are
superposed over its unit-gap solves (both stage 5); contact with perfect ground is solved as the doubled
image problem (a top-hat or folded monopole) — but see FND-191 for the H-shaped
image of a top-hat; every straight section at least two segments. Anything else keeps the per-path fallback and its warning, which
names the one-segment run or the wire to split. (A one-segment run with its current
carried unchanged across it was measured: 10.5 % off nec2c at 81 segments on a
folded dipole's jumpers, against 0.9 % with two-segment jumpers — so it stays
refused.)

Feedpoint error against nec2c 1.3.1 at 14.2 MHz:

| deck | segments per wire | error |
|---|---|---|
| stem-fed Y (stem 0..3 m, arms to (±2, 0, 5)) | 11 / 21 / 41 | 3.19 → 1.68 → 0.74 % (before: −1.83 − j1673 Ω; nec2c 23.66 − j1756) |
| stem-fed T (4 m stem, 3 m bar halves) | 11 / 21 / 41 | 6.45 → 2.23 → 1.33 % |
| dipole with a 2 m centre stub, fed off-centre | 11 / 21 / 41 | 9.26 → 5.11 → 2.94 % |
| 1 λ square loop (side 5.278 m) fed mid-side | 11 / 21 / 41 | 1.84 → 1.05 → 0.57 % (before ≈ 17 − j1163; nec2c 111.01 − j146.27) |
| loop with a 2 m stub on its top side | 11 / 21 / 41 | 1.65 → 1.03 → 0.57 % |
| T fed on the segment touching its node | 13 / 25 / 49 | 12.4 → 7.0 → 4.1 % |
| square loop 3 m over `GN 1` | 13 / 25 / 49 | 1.42 → 0.81 → 0.44 % |

Gate: `crates/nec_solver/tests/graph_nec2c.rs`.

## Still open (FND-162)

- **A straight run only one segment long.** Its single row cannot fix two
  constants, so the whole deck falls back to one term per path and its bends go
  unmodelled: about 26 % off in the currents on a U whose middle wire is one
  segment. Hallén warns on exactly these decks, driven or receive, through the same
  predicate the layout uses (`hallen_leaves_a_bend_unmodelled`).
- **Junction and loop decks outside the graph scope** — a one-segment section
  (measured and
  kept refused; the warning names it). These keep the per-path fallback, are
  warned about, and are pointed to `--solver mpie` where it takes the deck.
  Lumped loads, current sources, networks, plane waves (receive; with `LD` loads
  refused) and perfect-ground contact are in scope since stage 5, and a wire
  end or a crossing at another wire's interior joint is a connection (FND-192).
- **Receive over ground** was FND-170, not a bend problem: the receive forcing had
  no ground-reflected wave, so even a straight dipole over perfect ground was about
  55 % off. Over perfect ground it now has it, and the bend's corner term takes the
  ground images on the receive side too (an inverted-V over `GN 1`: 7.95 → 4.61 %
  at 21 → 41 per arm), and over finite ground with nec2c's Fresnel coefficients.

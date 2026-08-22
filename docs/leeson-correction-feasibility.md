---
project: fnec-rust
doc: docs/leeson-correction-feasibility.md
status: living
last_updated: 2026-08-22
---

# Leeson stepped-diameter correction — feasibility & design (BL-IMPR-014)

> **RESOLVED — IMPLEMENTED 2026-08-22.** The book was obtained. The algorithm is
> transcribed directly from Leeson, *Physical Design of Yagi Antennas*, ch. 8
> § 8.4 (Eqs 8-44…8-59, Tables 8-2/8-3) and implemented in
> `crates/nec_solver/src/taper.rs` (`leeson_equivalent_element`) plus the
> `fnec taper` CLI subcommand. It is **validated against the book's own worked
> example** (Table 8-3: a 100-unit half-element of 0.8/0.4-diameter sections →
> equivalent cylinder ℓ′ = 95.70, d′ = 0.594) to the digit, and the book's § 8.5
> shows the method tracking MININEC across the full taper range. The two blockers
> below (book-locked algorithm, no oracle) are both cleared: the book *is* the
> algorithm, and its worked example *is* the oracle. Everything below is the
> original feasibility analysis, retained as context.

**Original outcome (2026-08-22, superseded): researched, not implemented.** The
exact Leeson algorithm is book-locked and there is no ground-truth oracle in this
repo to validate a stepped-diameter correction against, so shipping a numerical
"correction" now would violate the project's validation standards (see
[[fnec-validation-strategy]] / `docs/evidence-tiers` discipline). This document
records what the correction is, the empirical evidence that fnec needs it, the
public options and their sourcing status, the validation blocker, and a
recommended implementation path once a reference is in hand.

## 1. The problem, demonstrated in fnec

Real HF Yagi/beam elements are built from several telescoping tubing diameters
(thick at the centre, thin at the tips). NEC-2-class cores mis-model such
stepped-diameter elements — over-reporting gain and under-reporting feed
impedance. fnec is no exception; measured 2026-08-21 on a 10 m dipole at 14 MHz
(tips 5 mm radius, centre 12.5 mm radius):

| Model | `--solver mpie` feed Z | note |
|:------|:-----------------------|:-----|
| Uniform 5 mm (thin) | 64.50 − 50.28j Ω | |
| Uniform 12.5 mm (thick) | 66.65 − 24.33j Ω | ~26 Ω less reactance — diameter matters |
| **Stepped** (thin tips + thick centre) | **64.54 − 50.83j Ω** | ≈ identical to the all-**thin** case |

The MPIE reduced kernel collapses the whole element to the *first* wire's radius
(with a mixed-radius warning), so the thick centre is ignored; the Hallén solver
instead breaks at the radius-change junctions (−4 Ω negative resistance). Either
way fnec's answer for a tapered element is wrong.

## 2. What the Leeson correction is

Dr. David Leeson (W6QHS), *Physical Design of Yagi Antennas* (ARRL, 1992), ch. 8,
gives a procedure that replaces a tapered-diameter element with an **equivalent
uniform-diameter substitute element** — a single diameter **and** a corrected
length — chosen so the substitute has essentially the same self-impedance as the
tapered element near resonance. It is a **geometry-preprocessing** step: the
solver is unchanged; it just sees the substitute element. Valid only for linear,
essentially unloaded elements within ~±15 % of self-resonance (sinusoidal current
assumption), with no junction loads. EZNEC and NECWin Plus implement it.

The method has two coupled parts:

1. **Equivalent diameter** — a current-weighted blend of the section diameters.
2. **Length correction** — because a tapered element "acts short" (the thin tips
   dominate the effective length), the uniform substitute must be *physically
   shorter* than the tapered element to resonate at the same frequency.

Part 2 (the length correction) is the non-trivial, book-specific piece.

## 3. Sourcing status

The **exact** Leeson equations are **not reproduced in any public source** we
could find — L. B. Cebik's definitive tutorials ("Tapering to Perfection",
"Stepped-Diameter Correction and Autosegmentation") describe the method
conceptually and explicitly defer the derivation to Leeson's 1992 book. Confirmed
2026-08-22 against `antentop.org/w4rnl.001/amod10.html`, `on5au.be` (taper), and
the Wikipedia *Antenna equivalent radius* article (which covers only the
*cross-sectional* equivalent radius, not the along-length stepped case).

Public, citable alternatives / building blocks:

- **Lawson, W2PV**, *Yagi Antenna Design* (1976) — an earlier stepped-diameter
  correction (different system).
- **Beezley, K6STI** — NEC-Wires substitution algorithms.
- **Schelkunoff equivalent radius** (textbook: Balanis; Stutzman & Thiele) — the
  current-weighted log-average of the section radii; the *diameter* part of the
  problem, without Leeson's length correction.
- **Macher (2011)**, "Radius correction formula for capacitances and effective
  length vectors of monopole and dipole antenna systems", *Radio Science* 46 —
  a peer-reviewed closed-form treatment of effective length + equivalent radius
  for stepped monopole/dipole systems (paywalled; a strong basis if obtained).

## 4. Candidate implementable core (equivalent radius)

The equivalent-radius part is implementable from public theory. For a centre-fed
dipole of half-length `h` with an assumed sinusoidal current
`I(z) = sin(k(h − |z|))` and piecewise-constant radius `a(z)`:

```
ln(a_eq) = ∫₀ʰ w(z)·ln(a(z)) dz  /  ∫₀ʰ w(z) dz
```

with the weighting `w(z)` taken as the current magnitude `I(z)` (some references
use `I(z)²`). **This weighting choice is unresolved from the accessible sources**
and materially changes the result — which is one reason not to ship it blind.
Modelling a uniform element of radius `a_eq` at the *same* physical length is a
first-order approximation: it captures the impedance-level/bandwidth effect of
diameter but **not** Leeson's resonant-length correction, so it under-corrects the
"acts short" frequency shift.

## 5. The validation blocker

fnec's validation philosophy is shape/reciprocity/consistency, not absolute
parity ([[fnec-validation-strategy]]) — but a stepped-diameter *correction* is an
**absolute-accuracy** claim, and there is **no oracle for it in this repo**:
`nec2c` and MININEC are themselves wrong for stepped elements (that is the whole
point of the correction), and fnec has no measured-antenna corpus. Without one of

- a **worked numeric example from Leeson's book** (input tapered dimensions →
  published equivalent uniform diameter/length), or
- a **measured** tapered-element resonance/impedance,

any implementation can be checked only for *self-consistency* (uniform input →
identity; result brackets the all-thin / all-thick uniform cases; monotonic in
the taper) — not for correctness. That is too weak a bar to wire an EM correction
into the solver and present its numbers as trustworthy.

## 6. Recommendation

1. **Obtain a validated reference** — Leeson (1992) ch. 8 worked example, or a
   measured tapered element. This unblocks everything.
2. Implement the correction as a **geometry-preprocessing utility** (a
   `leeson`/`taper` transform, or a `--taper-correct` step) that reads a
   stepped-diameter element spec and emits an equivalent uniform-diameter `GW`;
   the solver stays untouched.
3. Validate against the reference from (1), plus the self-consistency checks
   above; gate it with a corpus fixture.
4. Ship it clearly scoped to its domain (linear, ~unloaded, ±15 % of resonance,
   no junction loads) — a no-op for uniform-diameter antennas (the entire current
   corpus, dipoles, the unun vertical).

Until step 1 is in hand, the honest interim guidance for users modelling tubing
elements is the one already in the backlog: fnec cannot model stepped diameters —
approximate the element with a single representative diameter and treat the result
as indicative only.

## References

- Leeson, D. B. (W6QHS), *Physical Design of Yagi Antennas*, ARRL, 1992 — ch. 8.
- Lawson, J. L. (W2PV), *Yagi Antenna Design*, ARRL, 1986.
- Cebik, L. B. (W4RNL), "Tapering to Perfection" and "Stepped-Diameter Correction
  and Autosegmentation" (antenna-modeling series).
- Macher, W. (2011), *Radio Science* 46, RS4001 — radius correction / effective
  length for stepped dipole/monopole systems.
- Balanis, C. A., *Antenna Theory: Analysis and Design* — equivalent radius of
  nonuniform wires.

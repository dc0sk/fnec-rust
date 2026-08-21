---
project: fnec-rust
doc: docs/external/pymininec.md
status: living
last_updated: 2026-08-21
---

# Reference: pymininec (schlatterbeck/pymininec)

External reference captured for **inspiration and cross-validation**, not a
dependency. Recorded 2026-08-21.

- **Repository:** <https://github.com/schlatterbeck/pymininec>
- **Author:** Dr. Ralf Schlatterbeck (Open Source Consulting)
- **License:** MIT (permissive — ideas and, with attribution, code/test vectors
  may be reused; fnec itself is under its own `COPYING`).
- **Language:** Python 3 + NumPy/SciPy.
- **Companion:** `plot-antenna` (matplotlib/Plotly visualisation: elevation /
  azimuth / 3-D / VSWR, HTML export).

## What it is

A modern, vectorised Python rewrite of the 1980s **MININEC3** BASIC code
(Mini-Numerical Electromagnetics Code, US Navy / NOSC). It computes feedpoint
impedance, current distribution, and near/far fields by the **method of
moments** — but in the **MININEC formulation**, which is *different from NEC-2's*
(different basis/weighting and Green's-function handling). This distinction is
the single most useful thing about it for us: it is an **independent third
formulation** alongside fnec's Hallén/MPIE solvers and the `nec2c` oracle.

## Formulation notes

- MoM with pulse-on-segment unknowns. A wire of *N* segments carries *N−1*
  "pulses" (the interior connection points that hold current/loads); junctions
  are handled by explicit topology bookkeeping.
- Impedance matrix via Gaussian quadrature; self/near terms use elliptic
  integrals (now `scipy.special.ellipk`, which fixed typo'd hard-coded
  coefficients in the original BASIC).
- Segmentation guidance: segment length ≤ λ/20; endpoint coincidence matched
  within 1/1000 of the smallest segment (same spirit as NEC's fuzzy join).

## Capabilities (breadth worth noting)

- **Geometry:** straight wires with tapering (unequal segment lengths), **arcs**
  (inscribed-polygon approximation), **helices/spirals** (optional radius
  taper), plus geometry **transforms** — rotate (X/Y/Z), translate, scale
  (optionally per-element).
- **Excitation:** multiple feedpoints, complex voltage.
- **Loads:** simple Z, **series RLC**, **trap** (series RL in parallel with C),
  **Laplace-domain** (arbitrary rational s-domain networks), distributed
  **skin-effect conductivity**, and **insulated-wire** loading (dielectric
  sleeve, per Wu 1961).
- **Ground:** perfect ground; **multilayer media** with linear or circular
  boundaries; **radial-wire** ground screens (count/radius); terrain via height
  variation.
- **Output:** far field (dBi or V/m), near field, feedpoint Z across sweeps,
  current distributions. Sweep = start / increment / step-count.
- **CLI ergonomics:** options readable from files (with comments), shortest
  unique-prefix abbreviation. It does **not** read NEC card decks — its geometry
  language is its own (it can emit BASIC-format input for its own regression).

## Validation approach (methodology to borrow)

- Regression against the **original BASIC** MININEC (versions 9/12/13), run
  through two independent BASIC interpreters (`pcbasic`, `Yabasi`) to separate
  algorithm bugs from interpreter/precision effects.
- 100 % statement coverage via `pytest`; fixtures use `.mini` (input),
  `.pout`/`.bout` (Python/BASIC outputs).
- **Literature benchmark antennas:** 7 MHz dipole, inverted-L, T-antenna, and a
  12-element Yagi — plus Lewallen's straight/bent-dipole examples and the
  Zeineddin near/far-field thesis study.

## Known accuracy character vs NEC-2 (so we compare fairly)

- MININEC's resonant frequency runs **slightly high** (it models an antenna as
  effectively a touch too long).
- Near/far-field magnitudes: the BASIC code reads slightly **high** vs NEC, the
  Python rewrite slightly **low** vs NEC — largely single- vs double-precision
  history.
- Thin-wire assumption violations (fat wires, tight junctions, ground-plane
  contact on helices) give non-physical impedance — same failure class we guard
  against in fnec.

## Ideas & inspiration for fnec

Ranked by value to us. See the whole-project gap review that accompanies this
note; several of these are logged there as backlog candidates, not commitments.

1. **Use pymininec as an independent cross-validation oracle.** Our validation
   strategy already avoids blind `nec2c` parity because fnec's Hallén result
   differs from `nec2c` *systematically* (see the validation-strategy memory). A
   MININEC-formulation code is a genuinely independent third data point: where
   fnec, `nec2c`, and pymininec **all** agree we have high confidence; where fnec
   sits between the other two, formulation bias — not a bug — is the likely
   cause. Concretely: add a small opt-in study (like `studies/`) that runs the
   same handful of canonical antennas through all three and tabulates R/X and
   pattern shape, keeping MININEC's known high-resonance bias in mind.
2. **Richer loads — trap and Laplace-domain.** fnec has LD0–LD5 (incl. the newly
   corrected LD5 skin-effect). A **trap** (RL‖C) and a general **Laplace/rational
   s-domain** load would cover trap-loaded verticals/Yagis and matching networks
   that users actually build. Natural extension of the existing `loads.rs`.
3. **Geometry primitives: arc and helix, plus scale/rotate transforms.** fnec
   supports `GW` and `GM` (move); arcs (`GA`) and helices (`GH`) plus a scale
   transform would let users model loops, quads, and helicals directly instead
   of hand-expanding them into wires.
4. **Radial-wire ground screen / multilayer ground.** Directly relevant to the
   vertical-over-ground and unun-vertical cases where fnec currently needs an
   explicit counterpoise (it forbids `z=0` contact). A radial-screen ground model
   would let a base-fed monopole be modelled the way hams actually build it.
5. **Insulated-wire loading (Wu 1961).** A per-segment dielectric-sleeve model —
   matters for insulated antennas and velocity-factor effects; complements the
   distributed skin-effect path.
6. **Named literature benchmarks in the corpus.** Adopt the same canonical set
   (7 MHz dipole, inverted-L, T, 12-el Yagi, Lewallen bent dipole) as named,
   documented fixtures so regressions cite a literature source, not just a prior
   fnec run.

## Cited references (from pymininec)

- Julian, Logan, Rockway (1982), *MININEC: A Mini-Numerical Electromagnetics
  Code*, NOSC TD 516 (ADA121535).
- Logan & Rockway (1986), *The New MININEC (Version 3)*, NOSC TD 938 (ADA181682).
- Lewallen (1991), *MININEC: The Other Edge of the Sword*, QST.
- Zeineddin (1993), *Numerical Electromagnetics Codes* (near-field accuracy
  thesis, Ohio University).
- Cebik (2003), antenna-modelling example series.
- Chipman (1968), *Theory and Problems of Transmission Lines* (skin effect).
- Wu (1961), *Theory of the Dipole Antenna and the Two-Wire Transmission Line*
  (insulated-wire model).
- Burke & Poggio (1981, 1996), NEC method documentation.

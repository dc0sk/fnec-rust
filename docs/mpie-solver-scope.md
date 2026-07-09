---
project: fnec-rust
doc: docs/mpie-solver-scope.md
status: living
last_updated: 2026-07-09
---

# MPIE solver — scope

A scoping plan for a **second solver** in fnec: a mixed-potential EFIE (MPIE) with a
subsectional (triangle) basis, alongside the existing Hallén hybrid. It is proposed
because a single solver retires **three** currently-deferred Phase-9 frontiers, each
of which the Hallén architecture structurally cannot reach:

1. **Sommerfeld ground currents/patterns** (PH9-CHK-006 Level 2) — correct near-ground
   currents, gain, and efficiency, not just feedpoint Z. *Validated*: the MPIE with
   Sommerfeld reflected potential kernels reproduces nec2c GN2 to ~5 % on R **and** X
   (`studies/sommerfeld-ground/efie_mpie_ground.py`).
2. **Degree-3 (T/Y) junctions** (PH9-CHK-002) — the entire-domain Hallén prototype
   diverged (R → wrong fixed point); the industry fix is KCL-in-the-basis subsectional
   MoM.
3. **Closed loops** (PH9-CHK-002) — a cyclic triangle chain needs no endpoint
   condition; it falls out of the same basis for free.

The Hallén solver stays the default and is untouched; the MPIE is opt-in
(`--solver mpie`) and initially routes only the classes Hallén cannot do (or an
explicit override for A/B testing), so there is **zero regression risk** to the
validated corpus.

## Why MPIE, and why it is de-risked

- **MPIE keeps the scalar potential explicit.** The Hallén reduction eliminates the
  scalar potential (folding it into the `C·cos(ks)` homogeneous term); the surface
  wave and the junction charge condition both *live* in the scalar potential, which is
  why Hallén cannot represent them. MPIE carries `A` and `Φ` separately.
- **KCL is satisfied by the basis topology.** Overlapping triangle bases spanning a
  junction make Kirchhoff's current law exact by construction — no explicit KCL row,
  and the mixed-potential junction charge terms cancel term-by-term. This is the
  textbook, provably-convergent formulation (NEC-2, RWG).
- **The hard physics is already validated in Python.** Free-space MPIE ≈ nec2c; the
  Sommerfeld reflected vector/scalar-potential kernels + the `−j` normalization
  reproduce GN2; the reflected dyadic and its fast 1-D reduction are validated Rust
  primitives (`sommerfeld::reflected_e_projected*`). The remaining work is
  *implementation*, not research.

## Architecture

- **Basis:** piecewise-linear (triangle) current, nodal unknowns on the wire graph;
  `∇·f = ±1/ℓ` charge pulses. Interior nodes carry a triangle; a degree-N junction node
  carries N−1 arm-pair "dipole" bases so KCL is automatic; free ends simply have no
  basis (I = 0). A closed loop is a cyclic chain with no endpoint node.
- **Formulation (Galerkin):**
  `Z_mn = jωμ₀/4π ∬ f_m·f_n (ŝ·ŝ') G^A ds ds' + 1/(jωε₀4π) ∬ (∇·f_m)(∇·f_n) G^Φ ds ds'`,
  `V_m = ∮ f_m·E^inc` (delta-gap → nodal). Free space: `G^A = G^Φ = e^{-jkR}/R` with the
  reduced kernel `R = √(Δ²+a²)`. Solve `Z·I = V` directly (square, symmetric — no
  normal equations, no Hallén constants).
- **Ground:** add the reflected kernels `G^A_refl = −j·S{R_TE}`,
  `G^Φ_refl = −j·S{(k₀²R_TE+kz₀²R_TM)/λ²}` (horizontal); the general dyadic uses the
  3-scalar set `V_TE=S{R_TE}`, `V_TM=S{R_TM}`, `U=S{(R_TE+R_TM)/λ²}`. `S{f}(ρ,d) =
  ∫(λ/kz₀) f J₀(λρ) e^{-jkz₀d} dλ`, evaluated with the validated sin/cosh substitution.
- **Coexistence:** new `crates/nec_solver` module (e.g. `mpie.rs`) + a `SolverMode::Mpie`
  wired through the CLI exactly like the existing `--solver` and `--ground-solver`
  flags. Far-field reuses the existing per-segment radiation sum on the recovered
  segment-midpoint currents.

## Phased increments (each independently gated)

Order chosen so each phase ships value and de-risks the next; effort is
focused-session estimates.

### Phase A — free-space MPIE core (2–3 sessions)
Assemble `Z` (triangle A-term + charge Φ-term, reduced kernel), delta-gap `V`, direct
solve; report feedpoint Z + currents.
- **Gate A1 (analytic/structural):** `Z_mn = Z_nm` to machine precision (Galerkin
  symmetry); short-dipole capacitive-X sanity.
- **Gate A2 (nec2c):** straight λ/2 dipole R within a few % of 79.35 Ω **with a
  mesh-refinement plateau** (`|R(2N)−R(N)| → 0`). Document the MPIE's own systematic
  offset (Python shows ≈6 % at N=40 — a discretization effect, not a fixed bias like
  Hallén's ~32 Ω).
- **Gate A3 (identity):** a collinear split dipole equals the single-wire result to
  ~machine precision (the split-recovers-single gate, MPIE analog).
- **Watch:** self/near-segment charge integrals (log-singular even with the reduced
  kernel at coincident segments) — reuse the `elem` self-term extraction; sign errors
  in the Φ term show as wildly wrong X with plausible R (catch at A1/A2).

### Phase B — junctions + loops (2–3 sessions)
Degree-N junction bases (N−1 arm pairs) + cyclic loop chains.
- **Gate B1 (the headline):** the Y-junction deck (nec2c 71.5 Ω), refinement sweep
  11→21→41 seg/arm — **monotone convergence, `|R(41)−R(21)| < 1 Ω`, within 5 % of
  71.5** (exactly what the Hallén prototype failed). Plus a T-junction reference.
- **Gate B2 (degree-2 regression):** inverted-V 30/45/90° R within ~5 % (cross-check
  vs the validated conductor-path numbers 55.5/39.0/42.1).
- **Gate B3 (loops):** small loop `R_rad = 20π²(C/λ)⁴` analytic (a few %); 1λ square
  loop mid-fed within ~5 % of nec2c 111 Ω.
- **Gate B4 (internal):** nodal KCL residual = 0 by basis construction (assert on
  topology, not solve output); transmit/receive reciprocity on the Y.
- **Watch:** junction-basis arm-orientation signs (pass A3 but fail B1 at ~2× error);
  touching-segment charge integrals under-integrated (refinement drift returns).
- **Ships:** replaces the `HighDegreeJunction` / `ClosedLoop` guards with routing.

### Phase C — far-field from MPIE currents (1 session)
Feed recovered currents into the existing radiation-pattern sum; verify gain.
- **Gate C1 (nec2c):** free-space dipole/junction pattern + gain vs nec2c (≤ ~0.1 dB).
- **Gate C2 (reciprocity):** `|I_feed|²/G_θ` constant across θ.

### Phase D — Sommerfeld ground *in* the MPIE (2–3 sessions)
Add the reflected potential kernels to the `Z` fill; DCIM-accelerate for the N²
matrix (direct per-element Sommerfeld integrals are too slow — fit `S{·}` as complex
images `Σ aᵢ G(complex_image_i)`, extract the Zenneck pole).
- **Gate D1 (the headline):** low horizontal λ/2 dipole GN2 — currents + pattern +
  feed Z vs nec2c at 0.05/0.025 λ (the Python probe hits ~5 % on R and X; match that).
- **Gate D2 (limits):** PEC (GN1) image cancellation (R → ~6 at 0.05 λ ✓ in Python);
  vertical dipole unchanged; high-ground (≥ 0.25 λ) unchanged.
- **Gate D3 (DCIM):** fitted kernel vs the direct-quadrature oracle (`efie_mpie_ground.py`)
  < 1 % over the (ρ,d) domain **including** the low-d/large-ρ pole corner.
- **Watch:** the `−j` reflected-Green's normalization (Sommerfeld identity `S{1}=+j·image`);
  the `G^Φ` small-λ cancellation and (for d→0) the electrostatic static-image tail.

### Phase E — general orientation + wiring + docs (1–2 sessions)
Full reflected dyadic (3-scalar set) for bent/vertical/mixed decks; `--solver mpie`
CLI; `card-support-matrix.md`, roadmap, corpus regression, traceability.

**Total:** ~8–12 focused sessions across the five phases. Phases A–C alone (no ground)
already retire the degree-3 and closed-loop frontiers.

## Key decisions to settle before Phase A

- **Triangle vs NEC-3-term sinusoidal basis.** Triangle (RWG-1D) is simpler and
  provably convergent; the 3-term sinusoidal gives closer absolute currents but far
  more machinery. **Recommend triangle** — the gates are radiation-resistance /
  convergence / reciprocity, which triangle meets, and it is the lower-risk build.
- **Direct vs iterative solve.** Direct (square symmetric) — the systems are small
  (N ≲ 10³). No normal equations.
- **Routing.** Default stays Hallén; MPIE routes the guarded topologies automatically
  and is available via `--solver mpie` for the rest (A/B testing / ground).

## Risks

| risk | mitigation |
|:-----|:-----------|
| self/near charge-integral singularity | analytic log extraction (reuse `elem`); gate at A1/A2 |
| junction-basis sign/orientation bugs | Gate A3 (split identity) then B1 (Y convergence) isolate them |
| DCIM fit fragility (Phase D) | validate vs the `efie_mpie_ground.py` direct-quadrature oracle; extract the Zenneck pole explicitly |
| far-field consistency with recovered currents | reuse the validated radiation sum; reciprocity gate C2 |
| scope creep / coexistence with corpus | MPIE is additive and opt-in; the Hallén corpus is never touched |

## What is already done (inputs to this build)

- `studies/sommerfeld-ground/efie_mpie_ground.py` — validated free-space + Sommerfeld
  MPIE (the reference oracle for Phases A and D).
- `sommerfeld::reflected_e_projected` / `reflected_e_projected_fast` — validated
  reflected dyadic (the E-field form; Phase D needs the potential form `S{·}`, a
  sibling of the same integrals).
- The degree-3 diagnosis and the fable MoM assessments (in
  `docs/ph9-chk-002-general-junction.md` and memory).

## References

- `docs/ph9-chk-006-sommerfeld-ground.md` — Sommerfeld ground, Levels 0–2, the
  validated MPIE result.
- `docs/ph9-chk-002-general-junction.md` — degree-2 solved, degree-3/loop deferred.
- `studies/sommerfeld-ground/` — the validated Python prototypes.

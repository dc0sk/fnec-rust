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

## DCIM — Phase 1 & 2 results (2026-08-22)

Following the project's prototype-first method, `studies/sommerfeld-ground/dcim_probe.py`
implements one-level DCIM and validates it against fnec's **exact** reflected
potential kernels (a Python port of `sommerfeld::reflected_potential_kernels`).
The machinery is **validated as correct**: sampling the spectral reflection
coefficient along the deformed contour `kz0 = k0(1 − jτ)`, a GPOF (matrix-pencil)
fit `g(kz0) ≈ Σ aᵢ e^{j kz0 bᵢ}`, and the Sommerfeld identity
`∫ (λ/kz0) J0(λρ) e^{−jkz0 D} dλ = j e^{−jk0 r}/r` together reproduce the exact
kernels with ~5–6 **complex images** `G ≈ Σ aᵢ e^{−jk0 rᵢ}/rᵢ`,
`rᵢ = √(ρ² + (d−bᵢ)²)`. Accuracy vs the exact quadrature (14.2 MHz, εr 13, σ 0.005):

| kernel | ρ ≤ 0.05 λ | ρ = 0.2 λ | ρ ≥ 0.5 λ |
|:-------|:-----------|:----------|:----------|
| **G_Φ** (scalar; carries the surface wave) | ~0.02 % | ~0.1–0.2 % | 1–7 % |
| **G_A** (vector) | ~5–9 % | ~20 % | 40–70 % |

`G_Φ` is pointwise-good; `G_A`'s far zone is the one-level-DCIM lateral-wave tail.

### Phase 2 — end-to-end validation (2026-08-22)

Pointwise kernel error turned out to be the **wrong metric**: the MoM Z-matrix only
uses the kernel for ρ ≤ the wire half-length (~0.5 λ), and the feedpoint impedance
is insensitive to the far-zone (ρ ≈ 2 λ) lateral-wave tail. `dcim_mom_validate.py`
drops the DCIM kernel into the **validated EFIE-MoM** (`efie_mpie_ground.py`,
unchanged) and compares feedpoint Z, DCIM vs the exact kernel vs nec2c GN2:

| height | exact-kernel MoM | **DCIM MoM** | nec2c GN2 | Δ(DCIM, exact) |
|:-------|:-----------------|:-------------|:----------|:---------------|
| 0.05 λ | 64.00 + j49.18 | **64.98 + j43.71** | 67.26 + j52.61 | **6.9 %** |
| 0.025 λ | 83.46 + j66.26 | **86.52 + j59.77** | 87.81 + j68.64 | **6.7 %** |

**~7 % end-to-end at both heights — inside fnec's ~5–8 % gate** (DCIM R is within
1.5–3.4 % of nec2c). Two things were decisive: **two-level DCIM** (near + far image
sets, `dcim_fit_2level`) and a **GPOF pole filter** (`|z| ≤ 1`) — dropping the
exponentially-growing, non-physical poles that otherwise blow the two-level fit up
(186 − j6185 Ω without it). The residual is the reactance (X ~13–17 % vs nec2c),
which explicit Zenneck-pole extraction should tighten further.

### Phase 2b — current distribution + Zenneck-pole analysis (2026-08-22)

`dcim_mom_validate.py` also compares the **current distribution** (not just the
feedpoint Z): the DCIM MoM reproduces the exact-kernel current to **~7 % L2** at
both heights with the correct symmetric, feed-peaked shape (0.05 λ: exact feed |I|
1.24e-2 vs DCIM 1.28e-2). So DCIM gets the physics — currents, not only Z — right.

The residual reactance is the **Zenneck surface-wave pole**. Located analytically:
the TM pole is `kz0_p = k0/√(1+εc) = (0.249 + 0.054j)·k0` (`λ_p ≈ 0.97 k0`), and it
sits on the **improper Riemann sheet** (`εc·kz0 + kz1 = 0` needs the `+Im` kz1
branch; the principal sheet gives the numerator zero, not the pole). Extracting it
(residue + surface-wave Hankel term, subtracted before the DCIM fit) is the
standard reactance-tightening refinement — **but it is not blocking: the DCIM is
already inside fnec's ~5–8 % gate on both Z and currents without it.**

### Refined phased plan

1. **Phase 1 (done):** DCIM fit machinery + constants validated pointwise.
2. **Phase 2 (done):** two-level + pole-filtered DCIM validated **end-to-end** —
   feedpoint Z within ~7 % of the exact kernel at 0.05 λ / 0.025 λ.
3. **Phase 2b (done):** current distribution validated (~7 % L2, correct shape);
   Zenneck pole located (improper sheet) — its explicit extraction is a documented
   refinement (not blocking; already at gate).
4. **Phase 3:** Rust port — a `dcim` module (GPOF + pole filter + complex images)
   slotted into `assemble_z_matrix_with_ground` via a complex-distance Green's
   kernel (`exp(−jk r)/r`, complex `r`), replacing the ρ-grid quadrature; gate
   feedpoint Z → currents → pattern vs nec2c GN2 (needs the reference tools back),
   and lift the Hallén `--ground-solver sommerfeld` path from feedpoint-Z to full
   currents.

## References

- `docs/ph9-chk-006-sommerfeld-ground.md` — physics derivation + DCIM design.
- `docs/mpie-solver-scope.md` — the MPIE near-ground currents/patterns scope.
- `studies/sommerfeld-ground/` — Python prototypes (`efie_mpie_ground.py`, etc.).
- `crates/nec_solver/src/sommerfeld.rs`, `crates/nec_solver/src/mpie.rs`.

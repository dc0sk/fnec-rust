---
study: Sommerfeld ground — horizontal-dipole surface-wave feasibility
branch: feat/ph9-sommerfeld-ground
date: 2026-07-09
author: DC0SK
---

# Sommerfeld ground — feasibility probe (PH9-CHK-006)

This study proves — **before** any Rust implementation — that a direct
Sommerfeld-integral reflected kernel for a **horizontal** dipole over a lossy
half-space reproduces nec2c's exact **GN2** near-ground impedance, including the
**surface-wave sign flip below 0.1 λ** that fnec's scalar-Γ (reflection-coefficient
/ GN0) model gets wrong. It converts the "hardest, high-risk" ground frontier into a
de-risked, clearly-scoped increment.

## Script

| File | Purpose |
|------|---------|
| `horizontal_dipole_sommerfeld.py` | **Level 0** — reflected E_x Sommerfeld integral (φ=0, x/x) + PEC self-check + induced-EMF ΔZ vs nec2c GN1/GN2/GN0 |
| `efie_mpie_ground.py` | **Level 2 VALIDATED** — a mixed-potential EFIE (triangle basis) with the Sommerfeld reflected vector/scalar-potential kernels in the impedance matrix reproduces nec2c GN2 (and PEC/GN1) to ~5% on R and X, currents included. Proves the full-EFIE route; the `-j` reflected-Green's normalization is essential |
| `level2_architecture_probe.py` | **Level 2 architecture probe (negative result)** — shows the cheap routes (Born iteration; E-field correction into the Hallén operator) do NOT reproduce GN2 currents; a rigorous EFIE-MoM or reflected-vector-potential+DCIM approach is needed |
| `fast_1d_reduction.py` | **Level 1 fast kernel** — the 1-D azimuthal reduction of the general dyadic (single radial J0/J1/J2 integral, ~100× faster); validated vs the 2-D oracle to ~1e-6 for all orientations. Mirrors Rust `sommerfeld::reflected_e_projected_fast` |
| `general_dyadic.py` | **Levels 1 & 2 foundation** — the full reflected half-space dyadic for arbitrary source/obs orientation + arbitrary offset (2-D spectral integral); PEC self-check to ~1e-6 for every orientation pair, and an end-to-end tilted-dipole reaction ΔZ vs nec2c GN2 (<10%). See the "Generalization roadmap" in `docs/ph9-chk-006-sommerfeld-ground.md` |

## The formulation

Derived via the plane-wave (angular) spectrum, azimuth reduced analytically to a 1-D
Sommerfeld integral over the radial spectral variable λ (independently cross-checked
against a Michalski–Zheng mixed-potential derivation):

```
E_x^refl(ρ,d) = (ωμ0/8π) ∫_0^∞ (λ/kz0) e^{-j kz0 d}
                  [ R_TE (J0(λρ)+J2(λρ)) − R_TM (kz0²/k0²)(J0(λρ)−J2(λρ)) ] dλ
```

with `d = z+z'`, `kz0 = √(k0²−λ²)` (Im ≤ 0), `R_TE = (kz0−kz1)/(kz0+kz1)`,
`R_TM = (εc·kz0−kz1)/(εc·kz0+kz1)`, `kz1 = √(kg²−λ²)`, `kg = k0√εc`,
`εc = εr − jσ/(ωε0)`. The horizontal dipole excites **both** TE and TM, and the
TE/TM polarization coupling (the delicate part) is captured by the `J0±J2` split. The
vertical dipole is pure TM and is already accurate with fnec's scalar Γ — the whole
gap is the horizontal case.

## Results (14.2 MHz, εr = 13, σ = 0.005, horizontal λ/2 dipole, ΔZ vs free space)

**PEC field self-check** — the reflected-field integral with `R_TE=−1, R_TM=+1`
reproduces the exact opposite-current image field to a few % (validates every
prefactor/sign; residual is uniform-grid sampling of the λ=k0 integrable
singularity).

**ΔZ via induced-EMF reaction integral (assumed sinusoidal current):**

| height | mine ΔR | nec2c GN2 (truth) | RCM / GN0 (≈ fnec today) |
|:-------|--------:|------------------:|-------------------------:|
| 0.25 λ | +10.4 | +11.0 | +11.6 |
| 0.10 λ | −16.7 | −19.2 | −27.0 |
| 0.05 λ | −8.3 | −11.6 | −32.4 |
| **0.025 λ** | **+10.8** | **+9.0** | **−24.3** |

The PEC ΔZ pipeline matches nec2c GN1 to ~7–8 %. The Sommerfeld GN2 result tracks the
truth across the sweep and — critically — **reproduces the sign flip at 0.025 λ**
(mine +10.8, truth +9.0), where the reflection-coefficient method gives a
wrong-signed −24.3. The residual (~20 % at the lowest height) is the assumed
sinusoidal current; the reaction integral is stationary in the current to first
order, so fnec's actual solved current would tighten it.

## What this means for production

The physics and kernel are proven. Two implementation routes (see
`docs/ph9-chk-006-sommerfeld-ground.md` and the solver-frontier assessment):

1. **Reaction ΔZ correction (recommended first increment).** Post-solve, add
   `ΔZ_sw = −(1/I0²) ∬ I(s)[E^r_Sommerfeld − E^r_scalarΓ] I(s') ds ds'` using the
   existing conductor-path currents. No solver surgery; directly targets the pinned
   GN2−GN0 table. Needs the reflected kernel above as an oracle (this script) plus a
   robust λ-quadrature (contour deformation past k0, Zenneck-pole extraction).
2. **DCIM (production speed / full generality).** Fit the reflected kernel as
   `Σ aᵢ·G(complex_imageᵢ)` + surface-wave pole; slots into fnec's
   `Γ·elem(image)` structure with a complex-distance Green's kernel.

Numerical hazards to handle in Rust (all flagged, none blocking): the integrable
branch singularity at λ=k0, the Zenneck/surface-wave pole in the R_TM term (this is
what carries the low-height correction), and the slow tail for small `d`.

## Regenerating the nec2c references

```
GW 1 21 -5.278 0 h 5.278 0 h 0.001    # horizontal λ/2 dipole at height h
GN {0|1|2} 0 0 0 13 0.005             # 0=RCM, 1=PEC, 2=Sommerfeld
FR 0 1 0 0 14.2 ; EX 0 1 11 0 1 0 ; XQ
```
Free-space reference: 78.85 + j44.70 Ω. Heights: 0.25/0.10/0.05/0.025 λ =
5.278/2.111/1.056/0.528 m.

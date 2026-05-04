---
project: fnec-rust
doc: docs/utd-feasibility-assessment.md
status: reference
last_updated: 2026-05-04
correction: ADA305743 corrected from "likely NEC-BSC" to Rousseau & Pathak TD-UTD (1996), confirmed via PDF review
---

# UTD and Canning Simply Sparse: Feasibility Assessment

**Prepared by**: Claude Sonnet 4.6 (AI assistant)  
**Date**: 2026-05-04  
**Triggered by**: User question on UTD extension value and implementation feasibility  

---

## References reviewed

1. **Lertwiriyaprapa, Pathak & Volakis (2007)** — "A Uniform Geometrical Theory of Diffraction for
   predicting fields of sources near or on thin planar positive/negative material discontinuities",
   *Radio Science* 42(6), doi:10.1029/2007RS003689. Ohio State ElectroScience Laboratory.
   (Full text behind paywall; metadata confirmed via CrossRef.)

2. **DTIC ADA305743** — P. R. Rousseau and P. H. Pathak, *"Time Domain Version of the Uniform
   Geometrical Theory of Diffraction"*, OSU ElectroScience Laboratory Technical Report 721564-3,
   February 1996. ONR-sponsored (Contract N00014-91-J-1013). Full text confirmed via PDF review.
   Develops TD-UTD formulations via the **Analytic Time Transform (ATT)**: transient scattering from
   arbitrarily curved PEC wedges (Ch. 3) and smooth convex PEC surfaces (Ch. 4).

3. **Canning & Rogovin, "A universal matrix solver for integral-equation-based problems" (2003)** and
   follow-on papers — the **Simply Sparse** method (also called Impedance Matrix Localization, IML):
   a basis-transformation technique to sparsify the MoM impedance matrix.

---

## Background: methods compared

### Method of Moments (MoM) — fnec-rust's current solver

MoM is a full-wave integral equation method. It discretizes the Electric Field Integral Equation (EFIE)
or Hallén equation into an N×N dense linear system. Key properties:

- **Accurate at all electrical sizes** — no frequency restriction.
- **Required for antenna quantities**: feedpoint impedance, segment currents, near-field coupling.
- **O(N²) memory, O(N³) direct solve**. At N = 500 this is manageable; at N = 5000 it becomes a wall.
- **Assumes explicit wire/surface discretization** — cannot handle electrically large structures
  without correspondingly large N.

### Uniform Theory of Diffraction (UTD)

UTD is a **high-frequency asymptotic ray method**, a refinement of Keller's Geometrical Theory of
Diffraction (GTD). Fields are modeled as bundles of rays that travel in straight lines and diffract or
reflect at geometric discontinuities (edges, wedges, curved surfaces). Key properties:

- **Efficient for electrically large structures** — no surface mesh needed; complexity scales with
  geometry complexity, not electrical size.
- **Invalid below approximately 2–5 wavelengths** across a structure — pure ray assumptions break down.
- **Cannot compute feedpoint impedance or segment currents** — ray methods give radiated/scattered
  fields, not antenna terminal quantities.
- **Models diffraction at edges, wedges, and surface discontinuities** — exactly the physics that
  Sommerfeld integrals and Fresnel approximations miss for finite ground planes and platform edges.

### Hybrid MoM-UTD

The standard architecture (used in NEC-BSC, FEKO hybrid mode, and others) couples both methods:

- **MoM region**: The antenna wire structure and any nearby small conductors → yields wire currents.
- **UTD region**: Large platform (aircraft, ship, vehicle, building, finite ground plane) → yields
  scattered/diffracted field contributions as additional impedance-matrix entries.
- **Coupling**: MoM wire currents radiate incident fields onto UTD objects; UTD scattered fields appear
  as modified excitation and mutual-impedance entries in the MoM system. The coupled matrix is:

  ```
  [Z_mm + Z_mm^{UTD}] I_m = V_m
  ```

  where `Z_mm^{UTD}` contains the UTD-mediated interaction paths (direct + single-diffracted +
  doubly-diffracted rays from each wire pair).

### Canning's Simply Sparse (Impedance Matrix Localization)

A MoM matrix acceleration technique, **not** a solver replacement:

- Applies a basis transformation `T` so that `Z' = T† Z T` is sparse (many near-zero off-diagonal
  blocks corresponding to spatially distant interaction pairs).
- Solves `Z' I' = V'` using sparse factorization (no fill-in variant exists for the IML case).
- Achieves sub-O(N²) storage and faster iterative convergence for large N.
- Orthogonal to UTD — it works entirely within the MoM wire framework.

---

## Assessment: can UTD add advantage to fnec-rust?

### Short answer

**UTD cannot replace MoM** for any of fnec-rust's core antenna modeling tasks, and is not applicable to
the primary use case (isolated wire antennas at HF/VHF/UHF). However, **hybrid MoM-UTD would add
genuine new capability** for a class of problems that fnec-rust cannot currently address at all: antennas
on or near electrically large platforms (vehicles, ships, buildings) and antennas over finite ground
planes with edge diffraction.

The specific AGU paper (Lertwiriyaprapa et al.) extends UTD to material discontinuities (including
negative-index materials). This is a more specialized contribution relevant only if fnec-rust ever targets
metamaterial antenna or antenna-on-radome problems.

### Where UTD hybrid would add real value

| Problem class | Current fnec-rust capability | What hybrid MoM-UTD adds |
|:--------------|:-----------------------------|:------------------------|
| Antenna over infinite PEC ground | Image method (GN=1) — exact | Nothing; image method is already exact |
| Antenna over infinite finite-conductivity ground | Fresnel approximation (GN0/GN2) | More accurate Sommerfeld + surface wave treatment |
| Antenna over **finite** ground plane (radials with edge) | Not supported — Fresnel assumes infinite extent | UTD models finite-ground edge diffraction correctly |
| Antenna on a metallic vehicle/platform | Not supported — MoM would require meshing entire platform | UTD handles platform as large scatterer; only wire antenna needs MoM |
| Antenna near a building corner/edge | Not supported | UTD wedge diffraction coefficient directly applicable |
| Antenna near a material-coated surface | Not supported | Lertwiriyaprapa et al. UTD coefficient applicable |

### Where UTD does NOT help fnec-rust

| Case | Reason UTD is not applicable |
|:-----|:-----------------------------|
| Isolated dipoles, Yagis, loops, verticals | Structure dimensions ≈ 1–10λ; UTD accuracy requires >> 5λ across the diffracting object |
| Feedpoint impedance computation | UTD provides far/scattered fields, not terminal quantities — MoM must always compute impedance |
| Segment current distribution | Ray methods give no segment-level current resolution — MoM required |
| Near-field coupling between closely spaced wires | UTD far-field assumption breaks down in the near-field zone |
| Sub-wavelength antenna elements (loading, matching) | LD loads, TL networks — entirely within MoM domain, UTD irrelevant |

### On the Lertwiriyaprapa et al. paper specifically

The closed-form UTD coefficients for thin planar material discontinuities are a **building block** for
future hybrid modeling, not a standalone capability. Their direct relevance to fnec-rust's current roadmap
is low, but they would become relevant if:

- Antenna-on-radome (dielectric-coated platform surface) modeling is targeted.
- Antenna near a frequency-selective surface (FSS) or absorber panel is targeted.
- Any negative-index or metamaterial environment is in scope.

These are not in the current roadmap. The paper should be filed as a reference for a hypothetical Phase 6
material-boundary extension.

### On the DTIC ADA305743 report (Rousseau & Pathak 1996 — TD-UTD)

ADA305743 is **not** a hybrid MoM-UTD code manual. It is a foundational theoretical report extending
frequency-domain UTD into the **time domain** using the Analytic Time Transform (ATT). Its scope:

- **TD-UTD for curved PEC wedges** (Ch. 3): ATT applied to Kouyoumjian–Pathak edge diffraction
  coefficients; slope diffraction included; handles arbitrary wedge curvature via local plane-wave
  decomposition.
- **TD-UTD for smooth convex surfaces** (Ch. 4): creeping-wave (Franz) modes in the time domain via
  ATT inversion of the surface diffraction series.
- **Analytic Time Transform (ATT)**: a one-sided transform that produces complex-analytic time
  functions from frequency-domain UTD fields; avoids the non-causal artifacts that arise from naive
  Fourier inversion of UTD expressions with their GO jump discontinuities.

**Relevance to fnec-rust**: TD-UTD targets transient/pulsed excitation (radar signatures, UWB,
EMP/HEMP analysis). fnec-rust is a frequency-domain CW antenna modeling tool — TD-UTD does not
apply to its core use cases. This document is a more specialized reference than the frequency-domain
Kouyoumjian–Pathak UTD, and less immediately applicable to a hybrid MoM-UTD antenna solver than
originally assessed.

Potential future relevance:

- If fnec-rust ever adds broadband sweep → time-domain conversion (e.g., IFFT of complex impedance
  and pattern sweeps), the ATT mathematical machinery is directly applicable.
- For any impulsive or transient near-field scattering analysis, TD-UTD would be the correct tool.

For the hybrid MoM-UTD architecture, the foundational frequency-domain references remain
Kouyoumjian & Pathak (1974) and the NEC-BSC documentation (a separate document not yet located).

---

## Assessment: large-N MoM acceleration options

The current dense O(N²) matrix assembly and O(N³) direct solve are practical up to approximately
N = 2000 segments on a developer workstation and N = 500 on Raspberry Pi class hardware. Beyond
those thresholds, memory and time become user-visible constraints. Three acceleration strategies are
compared here.

### Adaptive Cross Approximation (ACA)

**What it is**: ACA is a purely algebraic method for compressing the MoM impedance matrix Z. For
two groups of segments that are well-separated in space, the off-diagonal block Z_{IJ} (rows from
group I, columns from group J) has low numerical rank — because the Green's function kernel
e^{-jkr}/r is smooth for segment pairs far apart. ACA finds the factorization:

```
Z_{IJ} ≈ U · V^T      (rank r << min(|I|, |J|))
```

using an iterative pivot-selection procedure: it samples rows and columns of Z_{IJ} adaptively,
stopping when the approximation error falls below a tolerance. No physics knowledge is required —
ACA treats Z as a black-box matrix.

**Why it fits fnec-rust's MoM solver**:

- The dense Z assembly code in `nec_solver` already produces the correct matrix; ACA operates on
  top of it without changing the physics model.
- ACA compression can be added as a post-assembly step: assemble Z densely (existing path), then
  apply hierarchical ACA to compress far-field blocks. Near-field blocks (close segment pairs)
  are kept dense.
- The compressed representation (H-matrix) supports fast matrix-vector products O(N log N),
  enabling iterative solvers (GMRES, BiCGSTAB) to replace direct LU factorization for large N.
- Correctness is easy to verify: compress at different tolerances and compare against the
  uncompressed solution for the same deck.

**Expected scaling**:

| N segments | Dense Z memory | ACA-compressed | Direct LU time | GMRES + ACA time |
|:----------:|:-------------:|:--------------:|:--------------:|:----------------:|
| 500 | 4 MB | ~1 MB | < 1 s | negligible |
| 2 000 | 64 MB | ~8 MB | ~1 s | ~0.5 s |
| 5 000 | 400 MB | ~20 MB | ~30 s | ~5 s |
| 20 000 | 6.4 GB (unusable) | ~80 MB | not feasible | ~30 s |

(Estimates assume complex128; ACA compression ratio ~20× for typical thin-wire geometries.)

**Implementation prerequisites**: dense Z assembly must be validated and corpus-gated (Phase 1/2).
ACA itself is a Phase 5 addition to `nec_accel` or a new `nec_solver::compress` module.

**Recommended Rust crate**: no mature Rust ACA crate exists at the time of writing; implementation
from scratch is ~1 000 LoC for the core pivot loop and H-matrix arithmetic. Alternatively,
`ndarray` + a BLAS backend can be used for the rank-r factorization steps.

---

### Fast Multipole Method (FMM)

**What it is**: FMM is a physics-based hierarchical method. Segments are organized into an
oct-tree. Far-field interactions between well-separated groups are computed via multipole
expansions (outgoing) and local expansions (incoming). The matrix is never assembled explicitly;
instead, the matrix-vector product Z·I is evaluated directly in O(N log N) or O(N) time,
enabling large-scale iterative solvers.

**Complexity comparison**:

| Method | Memory | Matrix-vector product | Direct solve |
|:-------|:------:|:---------------------:|:------------:|
| Dense | O(N²) | O(N²) per iteration | O(N³) LU |
| ACA / H-matrix | O(N log N) | O(N log N) | O(N log² N) |
| FMM | O(N) | O(N log N) or O(N) | iterative only |

**Why harder than ACA**:

- Requires an explicit multipole expansion of the Green's function for 3D EM (vector spherical
  harmonics, Gegenbauer series, or the MLFMA translation operators). This is physics-specific
  code — not algebraic.
- Requires an iterative solver (GMRES is standard for EFIE; BiCGSTAB is an alternative). Direct
  factorization is not available.
- Convergence of GMRES on the EFIE/Hallén system can be slow without a preconditioner; adding a
  suitable preconditioner (diagonal, near-field ILU, or sparse approximate inverse) is additional
  engineering work.
- The implementation effort is 5–10× larger than ACA.

**Recommended path**: implement ACA first (algebraic, directly verifiable, lower risk), validate
against the full corpus, then evaluate FMM only if N > 5 000 becomes a practical need. ACA covers
the realistic problem sizes for fnec-rust's target hardware (workstation, RPi 4/5) through Phase 5.

---

### Canning's Simply Sparse (Impedance Matrix Localization)

The Simply Sparse / IML method applies a basis transformation T so that Z' = T† Z T is sparse,
then solves Z' I' = V' using sparse factorization.

**Assessment**: less suitable than ACA or FMM for fnec-rust:

1. **Not urgent at current problem sizes**: the corpus uses N ≈ 51–500 segments; the scaling
   wall is not yet a practical constraint.
2. **Harder to verify**: the basis transformation introduces a mapping between physical wire
   currents and transformed basis currents; subtle accuracy issues are harder to diagnose than
   in ACA (where the uncompressed dense result is always available for comparison).
3. **Less well-tested in practice**: ACA and FMM have been independently validated in multiple
   MoM codes (FEKO, Altair Feko, openEMS-ACA patches, MLFMA research codes); Canning IML has
   fewer independent implementations.
4. **No fill-free sparse factorization**: the "no fill-in" property holds only approximately
   for realistic geometries; in practice, the sparse solver still fills significantly.

**Recommendation**: evaluate alongside ACA at Phase 5, but do not prioritize over ACA. If ACA
delivers the required scaling, IML provides no additional benefit worth the implementation cost.

---

### Scaling threshold summary

| Hardware | Dense solve ceiling | ACA recommended from | FMM needed from |
|:---------|:-------------------:|:--------------------:|:---------------:|
| Developer workstation (64 GB RAM) | N ≈ 2 000 | N > 2 000 | N > 20 000 |
| Raspberry Pi 4 (4 GB RAM) | N ≈ 500 | N > 500 | N > 5 000 |
| Raspberry Pi 5 (8 GB RAM) | N ≈ 700 | N > 700 | N > 7 000 |

These are fnec-rust's planning thresholds for Phase 5 large-problem work.

---

## Architecture implications if hybrid MoM-UTD is pursued

### What must be added (Phase 5+)

1. **UTD geometry engine**: Representation of large-body objects as canonical shapes (flat polygons for
   wedge edges, cylinders for curved surfaces). Input could be a new deck card class (e.g. `SP` for
   scattering patch — currently marked OUT OF SCOPE but could be re-scoped for UTD canonical shapes).

2. **UTD diffraction coefficient library**: At minimum, Kouyoumjian-Pathak PEC wedge coefficients. Add
   Lertwiriyaprapa et al. material-boundary coefficients if material surfaces are in scope.

3. **Ray-tracing engine**: For each wire-segment pair, enumerate all significant ray paths (direct, singly
   diffracted at each edge, doubly diffracted). Spatial acceleration (BVH or grid) required for
   platforms with many edges.

4. **Impedance matrix modification**: Add `Z_mm^{UTD}` contributions into the existing Hallén/Pocklington
   matrix assembly. This is the narrowest integration point — the solver itself does not change.

5. **Validation corpus**: Platform-effect reference cases in `corpus/`. FEKO (hybrid mode) or NEC-BSC
   can generate ground-truth outputs for hybrid MoM-UTD validation.

### What does NOT change

- The Hallén MoM solver and matrix assembly.
- The parser (except possibly new geometry card syntax).
- The report contract.
- The corpus validation framework.

This clean separation means hybrid MoM-UTD can be implemented as an **optional layer** that modifies the
matrix fill step, with zero impact on the existing solver paths when UTD objects are absent.

---

## Recommendation summary

| Item | Assessment | Priority | Suggested phase |
|:-----|:-----------|:--------:|:----------------|
| Hybrid MoM-UTD for finite ground planes | High value — closes a real capability gap (finite ground, radials) | Medium | Phase 4–5 |
| Hybrid MoM-UTD for antenna-on-platform | High value — addresses a common professional use case (vehicles, ships) | Medium | Phase 5 |
| Lertwiriyaprapa et al. material-boundary UTD | Specialized — relevant for radome/metamaterial antenna work only | Low | Phase 5+ or later |
| DTIC ADA305743 (Rousseau & Pathak — TD-UTD) | Time-domain UTD; relevant only for transient/pulsed analysis or future broadband sweep → TD conversion | Reference | Phase 5+ or later |
| **Adaptive Cross Approximation (ACA)** | **Preferred first MoM acceleration step — algebraic, verifiable, ~1000 LoC core** | **Medium** | **Phase 5** |
| **Fast Multipole Method (FMM)** | **Gold standard for N > 20 000; higher implementation cost — pursue after ACA** | **Low** | **Phase 5+** |
| Canning Simply Sparse (IML) | Applicable to MoM scaling but harder to verify and less proven than ACA | Low | Phase 5 (evaluate alongside ACA, deprioritize if ACA suffices) |

### Concrete next step (pre-Phase 5)

Before Phase 5 GPU work begins, add a single planning document or section to `docs/architecture.md`
that:

1. Names hybrid MoM-UTD as a Phase 5 roadmap candidate for finite-ground and antenna-on-platform use cases.
2. Identifies the impedance matrix modification point (`assemble_z_matrix_with_ground` or a new
   `assemble_z_matrix_with_utd_bodies`) as the integration boundary.
3. References NEC-BSC (architecture TBD — accession not yet located), Kouyoumjian & Pathak (1974), and Lertwiriyaprapa et al. (2007) as
   the foundational UTD literature.
4. Names FMM and ACA as the preferred candidates for large-N MoM matrix acceleration over Canning IML.
5. Defines the minimum corpus case for hybrid validation: a thin-wire dipole next to a flat PEC plate,
   with a FEKO reference output.

This is a one-hour documentation task that preserves the architectural decision record without committing
implementation effort before Phase 1/2 are solid.

---

## References

- R. G. Kouyoumjian and P. H. Pathak, "A uniform geometrical theory of diffraction for an edge in a
  perfectly conducting surface," *Proc. IEEE*, vol. 62, no. 11, pp. 1448–1461, Nov. 1974.
- T. Lertwiriyaprapa, P. H. Pathak, and J. L. Volakis, "A Uniform Geometrical Theory of Diffraction for
  predicting fields of sources near or on thin planar positive/negative material discontinuities,"
  *Radio Science*, vol. 42, no. 6, 2007. doi:10.1029/2007RS003689.
- P. R. Rousseau and P. H. Pathak, "Time Domain Version of the Uniform Geometrical Theory of
  Diffraction," OSU ElectroScience Laboratory Technical Report 721564-3, February 1996.
  DTIC accession ADA305743. (Develops TD-UTD via Analytic Time Transform for curved PEC wedge and
  smooth convex PEC surface scattering; ONR Contract N00014-91-J-1013.)
- F. X. Canning and K. Rogovin, "A universal matrix solver for integral-equation-based problems," *IEEE
  Antennas Propagat. Mag.*, vol. 45, no. 1, pp. 19–26, Feb. 2003.
- F. X. Canning, "Inversion of sparse MoM matrices without fill in," 2007.
- J. M. Rius et al., "Multilevel matrix decomposition algorithm for analysis of electrically large
  electromagnetic problems in 3-D," *Microwave Opt. Technol. Lett.*, 2000. (FMM reference for comparison.)
- M. Bebendorf, "Approximation of boundary element matrices," *Numerische Mathematik*, vol. 86, pp.
  565–589, 2000. (ACA reference for comparison.)

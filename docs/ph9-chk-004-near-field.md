---
project: fnec-rust
doc: docs/ph9-chk-004-near-field.md
status: living
last_updated: 2026-07-05
---

# PH9-CHK-004: near electric and magnetic field (NE / NH cards)

## Requirement / change

Roadmap `PH9-CHK-004` (CP-003, PRT-003): near-field output. fnec computed the
far-field radiation pattern but had no near-field capability. This adds the `NE`
card — the electric field on a grid of observation points near the antenna, for
EMC / coupling / field-exposure analysis.

## Design

The near field is post-processing over the already-solved segment currents (like
the radiation pattern — no solver changes). Each segment is modelled as a Hertzian
current element `I·L·û` at its midpoint, using the **full** dipole field (the
1/r, 1/r² and 1/r³ terms):

```
E(P) = Σ_n (η·I_n·L_n / 4π)·e^{-jk r_n} · [
         (2 c_n / r_n²)(1 + 1/(jk r_n))·r̂_n
       + (jk / r_n)(1 + 1/(jk r_n) − 1/(k r_n)²)·(c_n·r̂_n − û_n) ]
```

with `d_n = P − mid_n`, `r_n = |d_n|`, `r̂_n = d_n/r_n`, `c_n = û_n·r̂_n = cosθ`.
The 1/r term was derived to reduce **exactly** to fnec's far-field convention
(`E_θ = −j(kη/4π)·F_θ·e^{-jkr}/r`), so the near field is consistent with the
radiation pattern at large range.

- `nec_solver::near_e_field(segs, i_vec, freq, points) -> Vec<NearFieldE>`.
- `nec_model::NeCard` + parser: `NE I1 NX NY NZ X0 Y0 Z0 DX DY DZ` (rectangular
  grid; `I1 = 0`). Spherical (`I1 ≠ 0`) is skipped with a warning.
- `nec-cli::solve_session::build_near_field_rows` generates the grid and emits a
  `NEAR_FIELD / X Y Z EX_RE EX_IM EY_RE EY_IM EZ_RE EZ_IM` report section.

### Magnetic field (NH card)

The `NH` card is the exact magnetic companion, reusing the same grid layout and
infrastructure. Each element's field is azimuthal about its axis:

```
H(P) = Σ_n (I_n·L_n / 4π)·e^{-jk r_n}·(jk / r_n)(1 + 1/(jk r_n))·(û_n × r̂_n)
```

`nec_solver::near_h_field` emits a `NEAR_H_FIELD / X Y Z HX_RE … HZ_IM` section.

### Accuracy boundary

The point-element model is accurate away from the wire surface. **Very close to a
conductor** (`r_n` ≈ the wire radius) it departs from NEC's extended thin-wire
kernel — a documented limitation. Fields at practical near-field distances
(fractions of a wavelength and beyond) are sound.

## Validation (`crates/nec_solver/tests/near_field.rs`)

- **Far-limit vs gain-derived far field** — at 200 λ the field is transverse
  (`E_r/E_θ < 0.01`) and its magnitude matches the independently gain-derived far
  field `|E_θ| = √(G·2η·P_in/4π)/r` to **0.02 %**. This validates both the
  formula and the absolute normalization.
- **Broadside polarization** — on the equatorial x-axis a z-dipole's E field is
  purely z-polarized (parallel to the wire), with `Ey = 0` by symmetry.
- **Magnetic far-limit** — at 200 λ the ratio `|E|/|H|` equals the free-space
  impedance `η = 376.73 Ω` (to 0.02 %) and `H` is azimuthal (transverse to `r̂`,
  purely `φ̂`), confirming the plane-wave `E ⟂ H ⟂ r̂` relationship.

## Test results

`cargo test --workspace`: **583 passed**, 0 failed (was 581; +3 near-field tests);
clippy clean. The `NEAR_FIELD` section only appears when an `NE` card is present,
so existing report contracts are unaffected. `docs/card-support-matrix.md` gains
`NE` and `NH` → Partial.

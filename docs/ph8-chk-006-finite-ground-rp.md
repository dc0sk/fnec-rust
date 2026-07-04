---
project: fnec-rust
doc: docs/ph8-chk-006-finite-ground-rp.md
status: living
last_updated: 2026-07-03
---

# PH8-CHK-006: radiation pattern over finite ground (Fresnel reflection)

## Requirement / change

Roadmap `PH8-CHK-006` (CP-002, PRT-001): advance the finite-ground modelling. The
concrete, bounded, high-value gap found: the **radiation pattern over finite
ground was computed as free-space** — only PEC ground received an image
contribution, so a horizontal or vertical antenna over real earth got a
free-space pattern with no ground lobe and no horizon null.

## Design

The finite-ground far field uses the standard **Fresnel reflection-coefficient**
approximation: the total field is the direct ray plus the ground-reflected ray,
where the reflected ray is the PEC image contribution scaled per polarization by
the complex Fresnel coefficients.

- Complex ground permittivity `ε_c = ε_r − j σ/(ω ε₀)`.
- At observation polar angle θ (from zenith = incidence angle from the ground
  normal):
  - `Γ_v = (ε_c cosθ − √(ε_c − sin²θ)) / (ε_c cosθ + √(ε_c − sin²θ))` (vertical)
  - `Γ_h = (cosθ − √(ε_c − sin²θ)) / (cosθ + √(ε_c − sin²θ))` (horizontal)
- Far field: `E_θ = E_θ_direct + Γ_v·(image θ)`, `E_φ = E_φ_direct − Γ_h·(image φ)`,
  using the existing PEC image geometry. This **recovers the PEC image exactly**
  for `Γ_v = +1, Γ_h = −1` (the PEC limit), and at grazing (θ → 90°) both Γ → −1,
  giving the horizon null.

The gain normalization integrates the **upper hemisphere** with the same Fresnel
far field (finite ground confines radiation above the ground, like PEC).

Implementation: `nec_solver::farfield` — `fresnel_coeffs`,
`far_field_components_fresnel`, and `integrate_power_for_ground`;
`compute_radiation_pattern` routes `GroundModel::SimpleFiniteGround` through them
and nulls θ > 90°.

## Validation

- **PEC limit** (`finite_ground_high_conductivity_limit_matches_pec`) — a finite
  ground with ε_r = σ = 1e8 reproduces the PEC pattern to < 0.05 dB. This checks
  the sign convention with no external reference.
- **nec2c shape** (`finite_ground_pattern_shape_matches_nec2c`) — horizontal
  dipole 10 m over average ground (ε_r = 13, σ = 0.005), elevation cut: the
  induced pattern peaks at θ = 45° and matches nec2c's shape to **0.053 dB**
  (max deviation, after removing a constant offset).
- **Horizon null** (`finite_ground_has_horizon_null`) — θ ≥ 90° is null.

### Directivity vs gain (the constant offset)

fnec reports **directivity** (relative to radiated power); nec2c's **gain**
includes the ground-loss efficiency. On average ground the two differ by a
constant **~1.3 dB** (≈ 75 % efficiency, `10^0.13`). The design-relevant
quantity — the pattern *shape* (lobe angle, nulls, relative levels) — matches to
0.05 dB. Absolute-gain parity would require the ground-absorbed power (a
near-field / Poynting computation); that is a documented follow-on.

## Test results

`cargo test --workspace`: **560 passed**, 0 failed (was 557; +3 finite-ground RP
tests); clippy clean. Existing PEC / free-space RP corpus cases are unaffected.

## Related issue — resolved

fnec's `RP` card parser previously omitted the standard NEC **XNDA (I4)** field
(it read 7 fields), so a canonical 8-field `RP` card mis-parsed θ0. **Fixed
2026-07-04**: the parser now accepts both the canonical 8-field form
(`RP mode Nθ Nφ XNDA θ0 φ0 Δθ Δφ`) and fnec's legacy 7-field form, distinguished
by field count; XNDA is parsed for portability but does not affect the angle grid.

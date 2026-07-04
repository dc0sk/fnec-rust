---
project: fnec-rust
doc: docs/ph9-chk-003-absolute-gain-ground.md
status: living
last_updated: 2026-07-04
---

# PH9-CHK-003: absolute gain over finite ground (radiation efficiency)

## Requirement / change

Roadmap `PH9-CHK-003` (CP-002, PRT-001). PH8-CHK-006 gave the correct pattern
*shape* over finite ground but reported **directivity** (relative to radiated
power), leaving a documented **~1.3 dB constant offset** from nec2c's **gain** —
the ground-loss efficiency. This closes that gap: the reported pattern over lossy
ground is now **gain**, matching nec2c's absolute dBi.

## Design

For a lossless antenna, gain = directivity. Over a lossy finite ground the earth
absorbs part of the delivered power, so

```
gain = directivity · η ,   η = P_radiated / P_input   (radiation efficiency)
gain_dBi = directivity_dBi + 10·log10(η)
```

- `P_radiated = (k²·η₀ / 32π²) · ∮|F|² dΩ` — the absolute radiated power from the
  far-field sum `F` used by `compute_radiation_pattern`, integrated over the upper
  hemisphere (ground-aware). `η₀ = μ₀·c` is the free-space wave impedance.
- `P_input = ½·Re(Σ V_m·conj(I_m))` — the real power delivered at the feed(s).
- `η = P_radiated / P_input`, clamped to `(0, 1]`.

The normalization constant `k²·η₀/32π²` was **validated empirically**: for a
lossless free-space λ/2 dipole it yields `η = 0.9996 ≈ 1` (gain = directivity), so
the constant is correct before it is trusted over ground.

Implementation:

- `nec_solver::farfield::radiation_efficiency(segs, i_vec, freq, ground, p_input)`
  → η (public).
- `nec-cli::solve_session`: for `SimpleFiniteGround`, computes `P_input` from the
  feedpoint rows and adds `10·log10(η)` to every pattern-gain column (the −999.99
  horizon-null sentinel is left untouched). Free-space / PEC are lossless (η ≈ 1)
  and are deliberately **not** adjusted, so their corpus gates are unchanged.

## Validation

- **Lossless → η ≈ 1** (`radiation_efficiency_is_unity_for_lossless`) — free-space
  and PEC dipole efficiency within 1 % of unity (gain == directivity).
- **Absolute gain vs nec2c** (`finite_ground_absolute_gain_matches_nec2c`) —
  horizontal dipole 10 m over average ground (ε_r=13, σ=0.005): η = 0.74,
  gain = directivity − 1.31 dB, and the full elevation cut matches nec2c's
  **absolute** gain to **0.06 dB** (previously only the shape matched, with the
  1.3 dB offset). End-to-end the CLI RP now reports gain within ~0.06 dB of nec2c
  at every angle (θ=45° peak: −0.067 vs −0.06 dBi).

## Test results

`cargo test --workspace`: **566 passed**, 0 failed (was 564; +2 efficiency tests);
clippy clean. Corpus PEC / free-space RP cases are unaffected.

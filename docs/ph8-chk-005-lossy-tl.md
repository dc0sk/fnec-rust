---
project: fnec-rust
doc: docs/ph8-chk-005-lossy-tl.md
status: living
last_updated: 2026-07-04
---

# PH8-CHK-005: lossy transmission line (`tl_type != 0`)

## Requirement / change

Roadmap `PH8-CHK-005` (CP-003, PRT-002): extend `TL` to the lossy form. Previously
`tl_type != 0` was parsed but ignored with a warning.

## Design

The lossless `TL` stamp already models a 2-port with
`Z11 = Z22 = −jZ0·cot(βℓ)`, `Z12 = Z21 = −jZ0·csc(βℓ)`. The lossy line is the exact
generalization to a **complex propagation constant** `γ = α + jβ`:

```
Z11 = Z22 = Z0·coth(γℓ)        Z12 = Z21 = Z0·csch(γℓ)
```

For `α = 0` these reduce **exactly** to the lossless `−jZ0·cot/csc` (since
`coth(jβℓ) = −j·cot(βℓ)` and `csch(jβℓ) = −j·csc(βℓ)`), so the lossless path is a
special case and stays byte-identical.

### Card parameterization (fnec convention)

NEC-2's `TL` card is lossless and has no loss field; fnec's card carries a single
spare float `F3`. So the lossy form (`tl_type != 0`) defines:

- `F3` = **total matched-line loss in dB** → `αℓ = F3·ln(10)/20` nepers.
- velocity factor = 1, so `βℓ = k·ℓ`.
- `Z0` is the (real) nominal characteristic impedance.

This is a documented fnec-specific convention (the standard card underspecifies a
lossy line). The lossless form (`tl_type == 0`) keeps `F3` = velocity factor.

Implementation: `nec_solver::build_tl_stamps` — the `tl_type != 0` branch builds
`γℓ = αℓ + jk·ℓ` and stamps `Z0·coth(γℓ)`, `Z0·csch(γℓ)` (with a near-singular
`sinh` guard). Wired end-to-end through the existing TL-stamp application in the
CLI; no separate wiring needed.

## Validation

Internal physical checks (`crates/nec_solver/tests/lossy_tl.rs`) — no external
reference, since NEC-2's `TL` is lossless:

1. **Lossless limit** — a lossy line with 0 dB loss stamps *identically* to the
   lossless line (velocity factor 1), to < 1e-9.
2. **Attenuation** — `|Z12|` (far-end coupling) falls monotonically with loss.
3. **Matched-line limit** — a very lossy line (60 dB) hides its far end, so the
   input `Z11 = Z0·coth(γℓ) → Z0` (≈ 50 Ω) to < 0.5 Ω.

## Test results

`cargo test --workspace`: **563 passed**, 0 failed (was 560; +3 lossy-TL tests);
clippy clean. The lossless corpus TL cases are unaffected. `docs/card-support-matrix.md`
`TL other` → **Partial**.

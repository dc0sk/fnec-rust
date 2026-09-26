---
project: fnec-rust
doc: docs/ph8-chk-005-lossy-tl.md
status: living
last_updated: 2026-09-26
---

# PH8-CHK-005: lossy transmission line (`tl_type != 0`)

> **Superseded (2026-09-26, FND-111/FND-123).** The card layout and the matrix
> stamp described below are **retired**. `TL` now uses NEC-2's layout,
> `TL I1 I2 I3 I4 F1 F2 F3 F4 F5 F6 [F7] [F8]`: F1 = Z0 (negative = crossed
> line), F2 = length (≤ 0 = straight-line distance between the segment centres),
> F3–F6 = shunt admittances at the two ends, and two fnec extensions — F7 =
> velocity factor (default 1) and **F8 = total matched-line loss in dB** (default
> 0 = lossless), which replaces the `TYPE≠0` + `F3` = loss convention below.
> `NSEG` and `TYPE` no longer exist; the old layout is refused at parse time with
> the NEC-2 rewrite in the error (a lossy `TYPE=1` card with F3 = loss dB becomes
> `… 0 0 0 0 1 <loss>`). The line is no longer stamped as Z-parameters into the
> Hallén matrix (that stamp was inert); it is a two-port network across the port
> gaps, in parallel, as in NEC-2, with Y11 = Y22 = coth(γℓ)/Z0 and
> Y12 = −csch(γℓ)/Z0, γℓ = αℓ + j·kℓ/vf (lossless: −j·cotθ/Z0, +j·cscθ/Z0);
> shunts add to Y11/Y22. Loss < 0, vf ≤ 0, Z0 = 0 or an electrical length that is
> a multiple of λ/2 are errors. See `docs/card-support-matrix.md`.

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

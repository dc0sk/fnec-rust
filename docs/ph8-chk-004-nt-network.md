---
project: fnec-rust
doc: docs/ph8-chk-004-nt-network.md
status: living
last_updated: 2026-09-26
---

# PH8-CHK-004: NT two-port network stamping

> **Superseded (2026-09-26, FND-111/FND-123).** Everything below is the
> historical design and is **no longer how fnec solves NT or TL.** The Y→Z
> stamp into the Hallén matrix described here was inert (a +0.37 Ω shift where
> nec2c moves the feed by +78.5 Ω). NT and TL are now two-port
> networks connected **across the port segments' gaps, in parallel** (NEC-2's
> model): the port voltage is the gap voltage, KCL holds at an undriven port, and
> at a driven port the source is in parallel with the network (the source
> current = segment current + network branch, used for input impedance/power;
> the `CURRENTS` table and far field use the wire current, as nec2c does). It is
> solved by superposition — the response to the real excitation plus a unit-gap
> response per undriven port, weighted by port voltages from a small linear
> system. NT supplies Y11, Y12, Y22 directly; several networks at a port add;
> both ports on one segment act as a one-port Y11+Y22+2·Y12; a zero-admittance
> NT opens the wire at its ports, as in NEC. Unusable cards (missing segment,
> fewer than 10 fields, non-numeric) are now **errors**, not warn-and-skip, as
> are networks with a non-Hallén solver or a plane-wave/current-source drive.
>
> Validation is now external: the `dipole-nt-tl-equiv-freesp-51seg` NT (a 2 m,
> 50 Ω line's Y between segments 20 and 32) gives fnec 63.35−j213.68 vs nec2c
> 64.51−j212.78 (the stamp gave 75.02+j42.29). Identity gates: a one-port NT of
> 1/Z_L equals `LD 4 Z_L` at the same segment (0.07% on a straight wire, 0.5% on
> a start-to-start split — the Hallén least-squares inconsistency, FND-118); a
> shunt NT across the feed gives exactly 1/(1/Z_ant+Y). The 7 NT/PT corpus decks'
> malformed filler `NT 1 1 26 1 1 26 50.0 0.0` was replaced by well-formed NT
> cards. See `docs/card-support-matrix.md` for current behaviour.

## Requirement / change

Roadmap `PH8-CHK-004` (CP-003, PRT-002): implement `NT` (two-port network)
runtime semantics by stamping the network's parameters into the Z matrix between
the two referenced segments, mirroring the existing `TL` stamp path.

## Design

NEC2 `NT` card layout:
`NT tag1 seg1 tag2 seg2 Y11r Y11i Y12r Y12i Y22r Y22i` — the network's
short-circuit **admittance** parameters (mhos), reciprocal (`Y21 = Y12`).

fnec's MoM system is in impedance form (`Z·I = V`) and the TL path stamps 2-port
**Z-parameters** into the matrix (a lossless TL contributes
`Z11=Z22=−jZ0·cot θ`, `Z12=Z21=−jZ0·csc θ`). An `NT` network is therefore stamped
by converting its admittance matrix to impedance parameters, `[Z] = [Y]⁻¹`:

```
det = Y11·Y22 − Y12·Y21
Z11 = Y22/det   Z22 = Y11/det   Z12 = −Y12/det   Z21 = −Y21/det
```

then stamped at `(i1,i1), (i2,i2), (i1,i2), (i2,i1)` exactly like `build_tl_stamps`.

Implementation: `nec_solver::build_nt_stamps(deck, segs) → (Vec<NtStamp>,
Vec<NtWarning>)`, reusing the TL path's `find_segment_index` for endpoint
resolution (including the `seg=0` → tag-center shorthand). Malformed cards (fewer
than 10 fields, missing/coincident endpoints, or a **singular** admittance matrix
that cannot be inverted) are skipped with an explanatory warning.

The `NtCard` currently stores `raw_fields: Vec<String>`; `build_nt_stamps` parses
them, so no parser/model change is needed for this increment.

## Validation

Internal **consistency vs the corpus-validated TL stamp** — no external
reference. An `NT` whose Y-parameters are the inverse of a lossless TL's
Z-parameters (`Y = Z_tl⁻¹`) must stamp the Z matrix **identically** to that TL,
because `[Y]⁻¹` inverts straight back to the TL's `[Z]`.
`crates/nec_solver/tests/nt_network.rs`:

1. **TL-equivalence** — a `TL` (Z0=50 Ω, ℓ=5 m) and an `NT` built from its
   equivalent admittance produce identical 2×2 stamps (entry-for-entry, |Δ|<1e-9).
2. **Singular admittance** — a rank-deficient `Y` (det≈0) is skipped with a
   `singular admittance` warning.
3. **Missing endpoint** — an `NT` referencing an absent tag is skipped with a
   `not found in geometry` warning.

## Test results

`cargo test --workspace`: **550 passed**, 0 failed (was 547; +3 NT tests); clippy
clean.

## Increment 2 — CLI wiring (2026-07-02)

NT cards now stamp the Z matrix end to end.

- **Application** (`nec-cli::solve_session`): after the TL stamps, `build_nt_stamps`
  runs and its stamps are added to `z_mat` (`add_to_entry`), exactly like TL.
  Malformed/unsupported NT cards emit their `build_nt_stamps` warning
  (deduplicated so repeated identical cards warn once).
- **Deferred warning removed**: the blanket "NT card support is currently
  deferred" warning (`warn_nt_card_deferred_support`) is gone; well-formed NT
  cards stamp silently, malformed ones warn from the stamp path.

**Corpus / test updates**: the pre-existing `dipole-nt-*` decks use a **malformed**
NT card (`NT 1 1 26 1 1 26 50.0 0.0` — 8 fields, tag 26 absent), so they now warn
`NT card ignored` and solve as free-space (impedance unchanged, 74.23+j13.9 — 78.83+j42.44 since FND-156); the
`parser_warnings` NT tests and those corpus contracts were updated from the
"deferred" substring to `NT card ignored`.

**New real fixture** `corpus/dipole-nt-tl-equiv-freesp-51seg.nec`: a well-formed
NT between segments 20 and 32 whose admittance equals a lossless TL (Z0=50 Ω,
ℓ=2.0 m). End to end it reproduces the equivalent **TL** deck's feedpoint
impedance to ~1×10⁻⁵ Ω (both 70.6327+j14.0088) — a corpus-level confirmation that
the admittance→Z stamp is correct through the full solve.

Results: `cargo test --workspace` **550 passed**, 0 failed; clippy clean.
`docs/card-support-matrix.md` NT → **Partial**.

## Staged delivery

1. **Stamp core** (#262) — `build_nt_stamps` (Y→Z→stamp), validated against the
   TL stamp.
2. **CLI wiring** (this increment) — apply stamps in `solve_session`; remove the
   deferred warning; real NT-equals-TL corpus fixture.
3. **Breadth** — non-reciprocal / general networks; interaction with TL/loads.

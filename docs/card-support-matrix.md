---
project: fnec-rust
doc: docs/card-support-matrix.md
status: living
last_updated: 2026-10-19
---

# NEC Card Support Matrix

This document is the canonical reference for supported, partial, and deferred NEC card behavior in fnec-rust.  It covers every card that appears in the corpus or is relevant to 4nec2 / NEC2 compatibility.

Legend: **Full** — complete implementation; **Partial** — accepted and produces useful output, but some semantics are deferred (see Notes column); **Deferred** — parsed for portability but ignored at runtime with an explicit warning; **N/A** — not applicable or not in scope.

## Geometry cards

| Card | Support | Notes |
|------|---------|-------|
| CM / CE | Full | Comment cards; preserved in parse, ignored at runtime |
| GW | Full | Wire-segment definition |
| GE | Full | Geometry end. `GE I1` decides only whether a wire end on the ground plane is joined to its image (`1`) or left open (`0`, `-1`), as nec2c reads it; it does not select a ground — the last `GN` card does. "On the ground plane" is nec2c's band, within 10⁻³ of the segment's length of z = 0; under `GE 1` such an end is snapped to z = 0 (FND-243). Refused, by name: a value outside `-1..=1` (nec2c reads other values by sign, which NEC-2 does not define); `GE 1` with no `GN` ground (nec2c then solves free space while joining z = 0 ends to an absent image; fnec read it as a perfect ground until FND-241); and a wire end on the ground plane under `GE 0`/`-1` or with no `GE` card, which nec2c solves as an open end whose answer does not converge (FND-242). Pinned against nec2c in `apps/nec-cli/tests/ground_flags_nec2c.rs` |
| GM | Full | Geometry move, NEC-2 semantics: I2 = NRPT (number of new structures, 0 = move in place), F7 = ITS (the tag whose first segment begins the affected suffix, in definition order; 0 = whole structure). Copies are cumulative and tagged `tag + k*ITGI`; an in-place move applies ITGI to the tags too; tag-0 segments are never retagged; an ITS naming no wire is refused, as nec2c does. Trailing fields may be omitted. A negative ITGI is refused by name: it yields negative tag numbers that fnec's unsigned tags cannot represent. A same-tag copy (ITGI = 0) is numbered and addressed as nec2c does — segment *n* of a tag is its *n*-th occurrence, so "tag 1 segment 77" is the centre of a 51-segment wire's copy — and is its own wire (FND-135). Pinned against nec2c in `crates/nec_solver/tests/gm_nec2c.rs` (captured 2026-08-30). One documented limit: a same-tag copy (ITGI=0) makes two segments share a (tag, index), and fnec addresses the first — nec2c can name the nth occurrence. No currently-working deck is affected; tracked as FND-135. Was wrong in two fields until FND-119 |
| GR | Full | Geometry repeat, as NEC-2 reads it (FND-247): `GR I1 I2` makes the structure so far occur `I2` times in all, the original included, 360°/`I2` apart about z, tags `+ I1` per occurrence (tag 0 kept, FND-248). NEC-2's card has no angle and nec2c ignores a third field; fnec accepts one that is 0 or 360/`I2` and refuses any other value by name, because fnec before 0.23.0 read `GR I1 k F1` as `k` *additional* copies `F1` degrees apart — the refusal gives that structure's spelling (`GR I1 k+1` or a `GM`). Also refused, as nec2c stops on them: `I2` below 1, fractional fields; and a negative `I1`. Pinned in `apps/nec-cli/tests/gr_card_nec2c.rs` |
| GS | Full | Scale the structure: `GS I1 I2 F1` multiplies the ends and radius of every segment defined so far — `GM`/`GR` copies included — by `F1`, as nec2c does; wires after it are not scaled, later `GM` moves are in the new units, two cards compound, and `I1`/`I2` are ignored. Refused, as nec2c stops on them: `F1` missing or not positive. Pinned in `apps/nec-cli/tests/gs_card_nec2c.rs`. A geometry card (`GW`, `GM`, `GR`, `GS`, `GE`) after `GE` is refused, as nec2c refuses it (FND-245) |
| GX | Full | Reflect the structure: `GX I1 I2`, `I2`'s digits `XYZ` selecting the planes (x → −x for X, and so on), run z, then y, then x, as nec2c's `reflc` does. Each pass appends a mirrored copy of every segment present, tagged `tag + I1` (tag 0 stays 0), and the increment doubles per pass. Refused by name: a segment in or crossing its plane (nec2c stops on it; fnec also refuses the small crossings nec2c lets through to overlapping copies), a negative or fractional field, `I2` outside 0–999. A z-reflection over a ground buries the copy and is refused as a buried wire. Pinned in `apps/nec-cli/tests/gx_card_nec2c.rs` |
| GA | Partial | A wire arc, as nec2c's `arc` builds it: `GA ITG NS RADA ANG1 ANG2 RAD`, `NS` segments on a circle of radius `RADA` about the origin in the plane y = 0, from `ANG1` to `ANG2` degrees measured from +x toward +z; move it with `GM`. **Solved on `--solver mpie` only**: every node is a bend, so every straight run is one segment long, which the Hallén geometry refuses by name (its per-wire fallback read a half circle 40 % off nec2c). Refused, as nec2c hangs on or builds them wrong: `NS` < 1, `RADA` ≤ 0, a zero span or one over 360°, a segment spanning 180° or more. Segment table pinned against nec2c's; impedance converges to nec2c (`apps/nec-cli/tests/ga_gh_card_nec2c.rs`) |
| GH | Partial | A helix or spiral, as nec2c's `helix` builds it: `GH ITG NS S HL A1 B1 A2 B2 RAD`, `NS` segments from z = 0 up |`HL`|, `x = a(z)·cos(2πz/S)`, `y = b(z)·sin(2πz/S)`, uniform (`A2 = A1`, `B1 = 0` reading as `A1`) or tapered linearly (`B2 = 0` reading as `A2`); `HL < 0` is the other handedness. Read as NEC-2 / nec2c reads it (NEC-4's layout is not detected). **Solved on `--solver mpie` only**, as `GA`. Refused: `S` ≤ 0 (nec2c hangs on 0), `HL` = 0 (the flat spiral, which nec2c builds as zero-length segments), negative radii, every radius zero, and the two spellings nec2c reads otherwise than NEC-2 defines — `B2` given with `A2 = A1` (nec2c ignores it) and `B1 = 0` in a taper (nec2c starts the helix flattened). Pinned as `GA` |
| GC | Full | A tapered wire, as nec2c's `wire` builds it: a `GW` of radius 0 and directly after it `GC I1 I2 RDEL RAD1 RAD2` (`I1`/`I2` ignored, as NEC-2 defines them), each segment `RDEL` times the one before (the first `L(1 − RDEL)/(1 − RDEL^NS)`), the radii geometric from `RAD1` to `RAD2`. Both solvers take each segment's radius; the Hallén solve models a radius change only where two wires join (on its section graph), so it under-models a taper along one wire and says so, naming `--solver mpie` (FND-252, PH10-CHK-003). Refused by name: a radius-0 `GW` without its `GC` next, a `GC` after anything else (nec2c's "GEOMETRY DATA CARD ERROR"), `RDEL`, `RAD1` or `RAD2` not positive, one segment given two radii (nec2c silently uses `RAD1`), a segment tapered under 2 radii or 3 µm. Each taper equals its written-out `GW` twin and approaches nec2c (`apps/nec-cli/tests/gc_card_nec2c.rs`) |
| GN type −1 | Full | Null-ground explicit free-space (same as omitting GN) |
| GN type 0 | Partial | Finite ground via a **normal-incidence scalar** reflection coefficient on the (correct-signed, PH9-CHK-006) image. Impedance is accurate (≈ Sommerfeld, ~10%, gated vs nec2c) for antenna heights ≥ ~0.2 λ; below 0.1 λ it is a reflection-coefficient approximation (no surface wave) and fnec **warns** — unless the exact **Sommerfeld surface wave** is enabled via `--ground-solver sommerfeld` for any straight wire — horizontal, vertical or tilted (PH9-CHK-006) — which reproduces nec2c GN2 incl. the low-height sign flip; bent or mixed geometry is declined with a warning and keeps the scalar-Γ result. Angle/polarization-dependent Fresnel (RCM) is deferred |
| GN type 1 | Full | Perfect-conductor (PEC) image method (correct-signed image, PH9-CHK-006). Wires standing on the ground plane (the ground-mounted monopole) are solved by explicit images on `--solver hallen` (FND-082); on that doubled structure they take voltage and current sources, `LD` loads, `TL`/`NT` networks (each mirrored between the image ports) and incident plane waves, and wires grounded at both ends or meeting at a ground point solve as section graphs. Refused, by name: a wire lying in the plane, a doubled structure no route takes (a one-segment run), a current source with a network (as everywhere, FND-123), and `LD` loads with a plane wave |
| GN type 2 | Partial | Aliases the GN0 scalar finite-ground path by default; the true Sommerfeld surface wave is available via `--ground-solver sommerfeld` for a single straight wire (collinear segments), horizontal / vertical / tilted, with one feed and no TL/NT network (feedpoint Z only, nec2c GN2, PH9-CHK-006; bent or side-by-side parallel geometry, several feeds and networks are declined with a warning and keeps the scalar-Γ result), or via `--solver mpie` for the full near-ground **currents/patterns** on **any wire above ground**, straight or bent, however close to it — the exact image over `GN 1` (FND-238), and over a finite ground that image scaled by `(εc − 1)/(εc + 1)` plus a tabulated Sommerfeld remainder (FND-239), checked against nec2c's converged value down to 0.05 m (and 2 mm for a vertical). Only a wire that reaches the `z=0` plane is rejected on the MPIE path |
| GN other | Refused | A type outside NEC-2's `-1..=2` is refused by name: nec2c solves it inconsistently (perfect-ground currents under a finite-ground pattern). Until FND-241 fnec solved it in free space with a warning. With several `GN` cards the last one is the ground, as in nec2c |

## Program-control cards

| Card | Support | Notes |
|------|---------|-------|
| FR | Full | Linear frequency sweep; all FR steps solved and reported |
| EN | Full | Terminates parse |

## Excitation cards

| Card | Support | Notes |
|------|---------|-------|
Excitation types follow canonical NEC2 numbering (see
`docs/ph8-chk-002-plane-wave-excitation.md`). Only type 0 is solved today; the
others are **recognized** (classified per NEC2 and given an accurate, category-named
diagnostic) but fail fast until their runtime semantics land in Phase 8 — they are
no longer silently treated as EX type 0.

| EX type 0 | Full | Applied-field voltage-gap source; `I2 = 0` makes `I3` an absolute segment number, as NEC-2 and every other card's reference read it (FND-282; fnec looked for a wire tagged 0 until then). supported across all solver paths (Hallen, pulse, continuity, sinusoidal, mpie). The default Hallen path solves **degree-3+ (T/Y) junctions** and **closed loops** on its section graph (FND-162 stages 2+3) when the deck has voltage sources (or an EX 4 current source), LD loads and TL/NT networks allowed, perfect-ground contact through the image problem, every straight section ≥ 2 segments; outside that scope it falls back per wire and warns. `--solver mpie` (PH9-CHK-007) also solves junctions and loops, and gives near-ground currents over Sommerfeld ground; it feeds the graph node nearest the driven segment |
| EX type 1 | Partial | Incident plane wave, linear polarization. **Solves** on `--solver hallen` for a single straight wire (receiving antenna → induced `CURRENTS`, no feedpoint); validated vs nec2c shape + reciprocity (PH8-CHK-002). Straight non-junctioned multi-wire (parallel arrays) supported; **degree-2 junctioned geometry** (bends, start-to-start / end-to-end splits, inverted-V) now solves on continuous conductor paths (PH9-CHK-002 receive side); since FND-162 a bend is modelled (per-section terms, bend rows, corner term) and the induced currents track nec2c at a bend (90° L 3.6 %, U 1.1 %), gated at two meshes and by the absolute reciprocity level. **Over perfect ground** the forcing carries the ground-reflected wave, as nec2c's does (FND-170; it was absent, and a straight dipole came out 55 % off): straight, bent and junction decks track nec2c within their own free-space error (a 10 m dipole 5.8 → 3.3 % at 42 → 84 segments, free space 5.6 → 3.2 %; a T 0.56 %). Over finite ground (`GN 0`, `GN 2`) the reflected wave takes nec2c's Fresnel coefficients; its ratio to the free-space current matches nec2c's to 0.16 % 3 λ up in both polarizations, and 2.4 % at 0.24 λ, where fnec's own finite-ground matrix model shows. Wires touching perfect ground (a ground-mounted monopole) solve on the doubled structure — exactly the free-space double lit from θ and from 180° − θ, and a monopole 4.64 → 2.70 % from nec2c at 21 → 41 segments; a wave from below the ground plane (θ > 90°), and a wire lying in the plane are refused, by name; loads solve, mirrored as the same load on the image (FND-197); degree-3+ (T/Y) junctions and closed loops solve on the section graph (FND-162 stage 5; with `LD` or `--loads-config` loads as columns of the graph system, FND-197), and `--solver pulse` fails fast. NTHETA×NPHI incidence-angle sweeps emit a `RECEIVE_PATTERN` (PH9-CHK-001) |
| EX type 2 | Partial | Incident plane wave, right-hand elliptic. **Solves** on `--solver hallen` for a single straight wire via the complex polarization vector (axial ratio F6); reduces to linear for a z-wire / AR=0; tilted-wire currents match nec2c (PH8-CHK-002). Non-junctioned multi-wire supported |
| EX type 3 | Partial | Incident plane wave, left-hand elliptic. Same as type 2 with opposite handedness. The legacy `--ex3-i4-mode` flag is an obsolete no-op |
| EX type 4 | Partial | Current source (fnec's NEC-4-flavoured *segment* current; NEC-2's own EX 4 is an elementary Hertzian source at a POINT in space, a different drive, so nec2c cannot arbitrate between the two here). Since FND-118 a current source is the unit-voltage delta-gap solve rescaled by `i0 / I_feed`, which is exact for a linear system — so `Z = V/i0` equals the voltage-source impedance BY CONSTRUCTION, on every geometry and over any ground, not merely to within a tolerance. Before that it had its own augmented solver and agreed only in free space (0.02%), diverging 6.5% over ground and 6.4% on a bent conductor. Straight and degree-2 junctioned geometry both supported, and degree-3+ (T/Y) junctions and closed loops on the section graph (FND-162 stage 5); `--solver pulse` fails fast, and two current sources are refused (FND-129). Anchored against the MPIE kernel over ground in `apps/nec-cli/tests/current_source_ground_anchor.rs`, since the two drives agreeing is now definitional and cannot serve as its own gate |
| EX type 5 | Partial | Voltage source (current-slope discontinuity). **Solves** as a voltage source via fnec's applied-field method — same result as type 0 (PH8-CHK-003). NEC's separate current-slope numerics (~6% different) are a documented non-goal |

## Load cards

| Card | Support | Notes |
|------|---------|-------|
| LD type 0 | Full | Series RLC: `Z = R + j(ωL − 1/(ωC))`. Every `LD` type is stamped on Hallén and, since PH10-CHK-004, on `--solver mpie` — there as the load times the segment's centre current, NEC's own model, projected on the triangle bases as the `EX` gap is |
| LD type 1 | Full | Parallel RLC: `Z = 1 / (1/R + 1/(jωL) + jωC)` |
| LD type 2 | Full | Series RLC per metre: type 0 with `R`, `L` and `C` each times the segment length Δ (Ω/m, H/m, F/m), as NEC-2 and nec2c define it. Until FND-280 fnec read it as a lumped series RL and ignored `C` |
| LD type 3 | Full | Parallel RLC per metre: type 1 with `R`, `L` and `C` each times Δ. Until FND-280 fnec read it as a lumped series RC |
| LD type 4 | Full | Series impedance (frequency-independent): `Z = R + jX` |
| LD type 5 | Full | Wire conductivity: the exact internal impedance of a solid round wire, `Z = dl · γ/(2π a σ) · I0(γa)/I1(γa)` with `γ = (1+j)/δ`, `δ = √(2/(ωμ0σ))` (`loads.rs`, `wire_internal_impedance`). It reduces to the DC resistance `dl/(σπa²)` at low frequency and to the surface impedance `dl(1+j)/(2πaσδ)` once δ ≪ a, as NEC-2's `LD 5` does; the row used to give `dl/(2π·a·σ)`, which is neither |
| LD other | Refused | Any other type — a negative one included (`LD -1` is NEC-2's "clear the loads of an earlier run", meaningless in fnec's single run and undefined after another LD, FND-292) — a type-5 load with σ ≤ 0, an LD naming no segment, or a range NEC-2 cannot read (see the field mapping) is an error naming the card (FND-161). It used to be skipped with a warning, which solved the antenna without the load |

### LD field mapping

`LD  type  tag  seg_first  seg_last  F1  F2  F3`

| Type | F1 | F2 | F3 |
|------|----|----|-----|
| 0 | R (Ω) | L (H) | C (F) |
| 1 | R (Ω) | L (H) | C (F) |
| 2 | R (Ω) | L (H) | — |
| 3 | R (Ω) | — | C (F) |
| 4 | R (Ω) | X (Ω) | — |
| 5 | σ (S/m) | — | — |

The segment fields are read as NEC-2 and nec2c's `load()` read them (FND-282): `seg_first=0` applies to all segments of the tag (`seg_last` is then not read); `seg_last=0` means the same as `seg_first`; **`tag=0` makes `seg_first`/`seg_last` absolute segment numbers** — NEC's count of the deck's segments in build order, across every wire and copy — so `LD 4 0 32 32 …` loads the 32nd segment built, whatever its tag, and `LD 4 0 0 0 …` loads every segment. Until FND-282 fnec read `tag=0` as "every tag" with the range applied per tag, which loaded segment 11 of both wires on a two-dipole deck where nec2c loads one. Refused by name: `tag=0, seg_first=0, seg_last≠0` (NEC-2 has no segment 0; nec2c reads past its arrays there), `seg_first > seg_last` (nec2c's DATA FAULT), and a negative type (`LD -1`, NEC-2's "clear an earlier run's loads", FND-292). The same rule reads `--loads-config` ranges and the `PT` card's segment selection.

## Network cards

| Card | Support | Notes |
|------|---------|-------|
| TL | Partial | NEC-2 layout `TL I1 I2 I3 I4 F1 F2 F3 F4 F5 F6 [F7] [F8]` (see field table). Solved as a two-port network connected **across the port segments' gaps, in parallel** (NEC-2's model), not stamped into the matrix: Y11 = Y22 = coth(γℓ)/Z0, Y12 = −csch(γℓ)/Z0 with γℓ = αℓ + j·kℓ/vf; the F3–F6 shunt admittances add to Y11/Y22; negative Z0 = crossed line. Lossy via the F8 extension (total matched-line loss dB). Validated vs nec2c: `TL 1 26 2 26 50.0 0.1` across two λ/2 dipoles 1 m apart → fnec 84.05+j28.65, nec2c 84.826+j31.131. fnec's retired layout (`… NSEG TYPE Z0 LEN [VF]`) is **refused** at parse time with the NEC-2 rewrite in the message (FND-111, FND-123) |
| NT | Partial | Two-port network given directly as Y11 Y12 Y22 (`NT I1 I2 I3 I4 Y11r Y11i Y12r Y12i Y22r Y22i`), solved across the port gaps in parallel, exactly like TL (FND-123). Ports are read as TL's: tag 0 makes the segment an absolute segment number (FND-282), and segment 0 is the tag's centre only with a tag. Several networks at one port add; both ports on one segment act as a one-port Y11+Y22+2·Y12. A zero-admittance NT opens the wire at its ports (KCL forces zero current), as in NEC. Validated vs nec2c: a 2 m, 50 Ω line's Y between segments 20 and 32 of a dipole → fnec 63.35−j213.68, nec2c 64.51−j212.78 |
| NE | Partial | Near electric field (PH9-CHK-004): Hertzian-element sum over the solved currents, emits a `NEAR_FIELD` section. Both **rectangular (`I1=0`) and spherical (`I1=1`)** grids are supported — spherical reinterprets the fields as `NX→R, NY→φ, NZ→θ` and maps to Cartesian, with point locations matching nec2c. Validated vs the far field at large range (0.02%); very-near-the-wire accuracy is out of scope. **Over PEC (GN 1)** every segment's image is summed — a ground-mounted monopole equals its free-space double to round-off, a vertical dipole 30 m out is within 5 % of nec2c (FND-201). **Over a finite ground (GN 0/2)** the reflected field is not modelled and the CLI warns |
| NH | Partial | Near magnetic field (PH9-CHK-004): azimuthal Hertzian-element sum over the solved currents, emits a `NEAR_H_FIELD` section. Rectangular and spherical grids supported (same convention as `NE`). Validated by the far-field `|E|=η·|H|` relationship. Same ground treatment as `NE` (PEC images; finite ground warned) |
| PT | Partial | Print control applied at runtime, as NEC-2 reads the card (PH9-CHK-004, FND-299): `I1 ≤ −1` suppresses the CURRENTS table; `I1 ≥ 0` with `I3 = 0` prints every segment, whatever `I1` and `I2` say (nec2c's rule); `I1 ≥ 0` with a range prints the segments `(I2, I3..I4)` select — the tag's own numbering, or absolute segment numbers when `I2` is 0 (FND-282), the same resolver as `LD`. `I1 = 1..3` are nec2c's receive-pattern print formats: fnec prints the selected segments in its own table and warns. Last PT card wins |

### TL field mapping

`TL  I1  I2  I3  I4  F1  F2  F3  F4  F5  F6  [F7]  [F8]` (NEC-2 layout)

| Field | Meaning |
|-------|---------|
| I1, I2 | Tag and segment of end 1. Tag 0 makes the segment an absolute segment number, as NEC-2 reads it (FND-282). Segment 0 with a tag is fnec shorthand for the tag's centre segment (the lower centre for even segment counts; a note is emitted); with tag 0 it names nothing and is refused |
| I3, I4 | Tag and segment of end 2 (same rules) |
| F1 | Characteristic impedance Z₀ (Ω). **Negative = crossed line** (180° reversal; negates Y12) |
| F2 | Line length (m). Zero or negative = the straight-line distance between the two segment centres (NEC-2 semantics) |
| F3, F4 | Shunt admittance at end 1, real and imaginary (S) |
| F5, F6 | Shunt admittance at end 2, real and imaginary (S) |
| F7 | fnec extension: velocity factor (default 1) |
| F8 | fnec extension: total matched-line loss in dB (default 0 = lossless) |

Missing trailing fields default to 0 as in NEC, except F7, which defaults to 1. `NSEG` and `TYPE` no longer exist.

**Retired layout.** fnec's old `TL t1 s1 t2 s2 NSEG TYPE Z0 LEN [VF]` is refused at parse time, and the error gives the NEC-2 rewrite — e.g. `TL 1 26 2 26 1 0 50.0 0.1 1.0` → `TL 1 26 2 26 50.0 0.1`; a lossy `TYPE=1` card (F3 = loss dB) → `… 0 0 0 0 1 <loss>`; a non-1 velocity factor → `… 0 0 0 0 <vf>`. Detection: ≥ 8 fields, field 5 a bare integer ≤ 16 and field 6 exactly `0` or `1`. A genuine NEC-2 card that happens to match is refused too; the message says to write Z₀ with a decimal point (e.g. `50.0`).

### TL / NT refusals

These are **errors, not warnings** (previously an unusable card was warned about and skipped, which solved a different antenna):

- any unusable TL/NT card: missing segment, fewer than 10 NT fields, non-numeric field, Z₀ = 0, velocity factor ≤ 0, loss < 0, electrical length a multiple of λ/2;
- a network with a non-Hallén solver (pulse / continuity / sinusoidal): "supported on --solver hallen only" (MPIE already rejected TL/NT);
- a network with a plane-wave or current-source drive: voltage (delta-gap) sources only.

The GPU-resident path declines any deck with TL/NT; such decks solve on the CPU.

## Output / post-processing cards

| Card | Support | Notes |
|------|---------|-------|
| RP | Full | Far-field radiation pattern; all RP grid points computed and included in the `RADIATION_PATTERN` report section. `XNDA` X-digit → `NORMALIZED_PATTERN` section; `XNDA` A-digit → `AVERAGE_POWER_GAIN` line (solid-angle-weighted mean gain = radiation efficiency over the full sphere, matches nec2c to <1%; a loaded free-space dipole 0.454 vs 0.458) (PH9-CHK-004). Gains are directivity × radiation efficiency over every ground, so lossy loads cost gain in free space and over PEC as in nec2c (FND-200). The `N`/`D` digits (gain-component labeling / dB-vs-ratio output-format toggles) are deferred — fnec always emits vertical/horizontal/total gain in dBi |

## Cards fnec does not model

Every NEC card fnec does not parse is listed here, with what it does (FND-236).
**Ignored** cards cannot change the solved problem: each is skipped with one
warning saying why. **Refused** cards would — skipping one solves a different
antenna (before `GS` was modelled, the feet-unit dipole of `gs_card_nec2c.rs`
with its `GS 0 0 .3048` skipped was a wire 3.28× too long: 227.14 + j451.81 Ω in
nec2c, where nec2c scaling it gives 79.042 + j45.909) — so the deck is refused with an error naming the card. A mnemonic
in neither list is refused as unknown, as nec2c refuses it. A line starting with
`'` is a comment. This table and `nec_parser::OTHER_CARDS` are checked against
each other (`crates/nec_parser/tests/card_matrix.rs`).

| Card | Treatment | What it is, and why |
|------|-----------|---------------------|
| XQ | Ignored | fnec always executes the deck, once, at its last `FR` card (an earlier `FR` is reported as replaced, FND-057); a pattern `XQ` asks for (I1 ≠ 0) is not produced — use an `RP` card |
| EK | Ignored | NEC's extended thin-wire kernel; fnec's kernel does not change, and NEC's thin-wire limit with `EK` (segment/radius > 2) is not fnec's |
| KH | Ignored | NEC's interaction-approximation range; fnec computes every interaction exactly |
| CP | Ignored | NEC's maximum-coupling calculation; its output is not produced |
| PQ | Ignored | Charge-density print control; fnec prints no charge densities |
| PL | Ignored | xnec2c's plot-file output; no plot file is written |
| WG | Ignored | Writes a Numerical Green's Function file; none is written |
| GF | Refused | A structure read from a Numerical Green's Function file |
| GD | Refused | Additional ground parameters: a cliff or a second medium |
| SP | Refused | A surface patch |
| SM | Refused | A set of surface patches |
| SC | Refused | A surface-patch continuation |
| NX | Refused | A further structure in the same file |
| SY | Refused | Symbol definitions — write the deck with `$NAME` templates and pass the values with `--vars` |

---

*See also*: [CLI guide](cli-guide.md) · [Design](design.md) · [Roadmap](roadmap.md)

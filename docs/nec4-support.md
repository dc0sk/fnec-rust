---
project: fnec-rust
doc: docs/nec4-support.md
status: living
last_updated: 2026-10-13
---

# NEC-4 Support Boundary

What fnec offers where NEC-4 goes beyond NEC-2, and where it stops. fnec reads
NEC-2 decks; it is not a NEC-4 implementation, and claims no NEC-4 parity.

**The status of each card is in [card-support-matrix.md](card-support-matrix.md)**,
the one canonical table, checked against the parser by
`crates/nec_parser/tests/card_matrix.rs`. This page used to carry two card tables of
its own, written for Phase 1, and they had drifted from the parser on almost every
row (FND-235); it carries none now.

## NEC-4 features and fnec's position

Each line was checked against the 0.21.7 binary on 2026-10-13.

| NEC-4 feature | fnec |
|:--------------|:-----|
| Segment current source (`EX 4` in NEC-4's sense) | **Solved** on `--solver hallen`: the specified current is forced and `Z = V/i0` is reported. fnec's `EX 4` is this NEC-4-flavoured segment current, not NEC-2's Hertzian point source. Details and limits: [card-support-matrix.md](card-support-matrix.md#excitation-cards) |
| Sommerfeld ground for wires near the ground | **Solved.** `--ground-solver sommerfeld` corrects the feedpoint impedance of a straight wire over `GN 2`; `--solver mpie` uses the exact Sommerfeld kernel for currents and patterns of any wire above the ground, straight or bent ([mpie-solver-scope.md](mpie-solver-scope.md)) |
| Wires touching or crossing a finite ground (ground stakes, buried radials) | **Refused**, naming the segment: no trustworthy contact model exists in fnec. A wire standing on *perfect* ground (`GN 1`) is solved by images |
| NEC-4-only cards (`IS` insulated wire and the rest) | **Refused** as unknown cards, as nec2c refuses them (FND-236) |
| Structured output | `--output-format json` writes one record per frequency point ([json-output-schema.md](json-output-schema.md)) |
| Larger problems | A dense solve on the CPU, or on a GPU where this host's calibration says it is faster ([cli-guide.md](cli-guide.md)) |

## Solvers

| Mode | Status |
|:-----|:-------|
| `--solver hallen` (default) | Production. The 51-segment λ/2 reference dipole is 79.33 + j45.04 Ω against nec2c's 79.35 + j46.22 on the same mesh |
| `--solver sinusoidal` | Production for straight wires and collinear chains, in any card order; refuses bends and junctions |
| `--solver mpie` | Production; junctions, loops and wires over ground, including the Sommerfeld kernel |
| `--solver pulse`, `continuity` | Unvalidated: refused unless `--experimental-solver` is given, and then every result carries a caveat (FND-080) |

## Out of scope

- Buried conductors and wires reaching a finite ground, as above.
- Surface patches (`SP`, `SM`, `SC`), refused by name.
- Symbol definitions (`SY`), refused by name. fnec's own `$NAME` templates with
  `--vars` take their place.

## References

- **NEC-2 Theory of Operation**: Burke & Poggio, LLNL 1981 (FORTRAN reference implementation)
- **xnec2c source**: https://github.com/KJ7LNW/xnec2c (working C reference)
- **4nec2 documentation**: https://www.4nec2.net/ (user-facing reference)

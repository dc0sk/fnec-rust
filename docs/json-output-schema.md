---
project: fnec-rust
doc: docs/json-output-schema.md
status: living
last_updated: 2026-09-08
---

# fnec JSON Output Schema (v1)

`fnec` can emit machine-readable JSON on stdout by passing `--output-format json`.
All text diagnostics and warnings continue to be written to **stderr**.

## Activation

```sh
fnec --output-format json <deck.nec>
fnec --output-format json --solver hallen <deck.nec>
fnec --output-format json --sweep-config sweep.toml <deck.nec>
```

## Top-level structure

The output is a JSON **array** — one element per frequency point solved, in
the same order the frequencies were resolved in — from the deck's `FR` card, or
from `--sweep-config` when that flag is given.

A deck with **no frequency at all** — no `FR` card and no `--sweep-config` — is
refused with exit 1 and writes nothing to stdout. It is not an empty array: `[]`
means *solved, and there was no feedpoint to price*, which is a different
outcome and must stay distinguishable from *never solved*.

Until v0.18.0 such a deck exited 0 with zero bytes, which `json.loads('')` turns
into an exception rather than an empty list (FND-084). The ledger's proposed
one-line fix — emit `[]` before the early return — was **not** taken: it applies
only to JSON mode, leaving text mode silent, and it would have spent the one
signal that already means something else.

Note that `--sweep-config` **supplies** the frequency list, so a deck with no
`FR` card solves normally when that flag is given; the refusal is over the
resolved list, not over the deck.

```json
[
  {
    "freq_mhz":   14.0,
    "tag":        1,
    "seg":        26,
    "z_re":       73.1,
    "z_im":       0.47,
    "z_abs":      73.1015,
    "z_arg_deg":  0.368
  }
]
```

### Field reference

| Field | Type | Unit | Description |
|:------|:-----|:-----|:------------|
| `freq_mhz` | `number` | MHz | Solved frequency. |
| `tag` | `integer` | — | Wire tag of the feedpoint segment (from EX card). |
| `seg` | `integer` | — | Segment number of the feedpoint. |
| `z_re` | `number` | Ω | Real part of feedpoint impedance (resistance). |
| `z_im` | `number` | Ω | Imaginary part of feedpoint impedance (reactance). |
| `z_abs` | `number` | Ω | Magnitude of feedpoint impedance: `sqrt(z_re² + z_im²)`. |
| `z_arg_deg` | `number` | ° | Phase angle of feedpoint impedance: `atan2(z_im, z_re)` in degrees. |

All numeric fields are IEEE 754 double-precision floating-point values.

### Multi-source decks

For decks with more than one EX card, only the first excitation source is
represented per frequency-point record.  This is the same source that
appears first in the `FEEDPOINTS` section of the text report.  Full
multi-source support (one record element per feedpoint per frequency) is
tracked under EP-4/EP-5.

### Absence of feedpoint data

A deck whose `EX` card names a drive that yields no priceable feedpoint — an
incident plane wave, for instance — solves and reports no feedpoint record: the
JSON array is empty (`[]`), no error is raised, and the exit code is 0.
Measured 2026-09-07 on `corpus/dipole-ex1-freesp-51seg.nec`.

(This paragraph used to open with the general claim that *any* deck producing
no sweep summary yields `[]` and exit 0. It does not hold for a deck with no FR
card — see the note under "Top-level structure" — so it is stated for the case
that was actually measured rather than for the class.)

**A deck with no `EX` card at all is not such a deck, and no longer reaches
this case.**  Until the change recorded in the changelog's next release it did:
`fnec` printed `[]` and exited 0, and in text mode printed a full `CURRENTS`
table of exact zeros, a `RADIATION_PATTERN` of `-999.9900`, and a `diag` line
reading `rel_res=0` — a structure that nothing drives, reported as a converged
solve.  It is now refused before the solve, on every frontend and every
`--solver` mode:

```console
$ fnec no-ex.nec
error: [validator] EX: this deck has no EX card, so nothing drives it and there is no solve — an undriven structure carries zero current everywhere. Add a driven source (`EX 0` or `EX 5`) to transmit, or an incident plane wave (`EX 1`, `2` or `3`) to receive
$ echo $?
1
```

Exit code 1, and **stdout carries nothing at all — not `[]`**.  An optimizer
loop that fed such a deck in and parsed the empty array back out now sees a
non-zero exit; it should report the error rather than record a null result,
which is what an empty array had been silently standing for.

## Stability guarantee

The field set listed above is **stable** as of schema v1.  New fields may be
added in future minor versions without changing the schema version number.
Field removals or type changes will increment `schema_version`.

Callers must tolerate unknown fields (standard JSON forward-compatibility
practice).

## Usage in optimizer loops

```python
import subprocess, json

result = subprocess.run(
    ["fnec", "--output-format", "json", "dipole.nec"],
    capture_output=True, text=True, check=True,
)
records = json.loads(result.stdout)
z = complex(records[0]["z_re"], records[0]["z_im"])
swr = (abs(z) + 50) / (abs(z) - 50) if abs(z) != 50 else 1.0
print(f"Z = {z:.2f} Ω  SWR@50Ω ≈ {abs(swr):.2f}")
```

See `docs/automation-guide.md` (PH4-CHK-006) for end-to-end optimizer
examples.

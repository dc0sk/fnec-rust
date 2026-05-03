---
project: fnec-rust
doc: docs/json-output-schema.md
status: living
last_updated: 2026-05-03
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
the same order as the deck's FR card defines them.  If the deck has no FR
card the array is empty (`[]`).

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

If a deck produces no sweep summary (e.g. a pattern-only deck with no EX
card) the JSON array will be empty (`[]`).  No error is raised; the exit
code is 0.

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

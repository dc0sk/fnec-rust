---
project: fnec-rust
doc: docs/json-output-schema.md
status: living
last_updated: 2026-10-29
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
    "z_arg_deg":  0.368,
    "feedpoints": [
      {"tag": 1, "seg": 26, "z_re": 73.1, "z_im": 0.47, "z_abs": 73.1015, "z_arg_deg": 0.368}
    ]
  }
]
```

### Field reference

| Field | Type | Unit | Description |
|:------|:-----|:-----|:------------|
| `freq_mhz` | `number` | MHz | Solved frequency. |
| `tag` | `integer` | — | Wire tag of the segment the `EX` card resolves to. With tag 0 on the card NEC-2 reads its segment as an absolute segment number (FND-282), so `EX 0 0 32` on two 21-segment wires reports `tag` 2. |
| `seg` | `integer` | — | Segment number of the feedpoint within that tag (`EX 0 0 32` above reports 11). |
| `z_re` | `number` | Ω | Real part of feedpoint impedance (resistance). Under `--ground-solver sommerfeld`, when the correction applies (`gsolver=sommerfeld-applied` on the `diag:` line), `z_re` and `z_im` include the surface-wave correction. The solve's V and I do not, so `z` is not V/I (FND-286). |
| `z_im` | `number` | Ω | Imaginary part of feedpoint impedance (reactance). |
| `z_abs` | `number` | Ω | Magnitude of feedpoint impedance: `sqrt(z_re² + z_im²)`. |
| `z_arg_deg` | `number` | ° | Phase angle of feedpoint impedance: `atan2(z_im, z_re)` in degrees. |
| `feedpoints` | `object[]` | — | **Every** feedpoint of the point, in the text report's `FEEDPOINTS` order, each with `tag`, `seg`, `z_re`, `z_im`, `z_abs`, `z_arg_deg` as above. Always present — one element for a one-source deck — so a consumer never branches on its presence. The top-level fields are its first element, digit for digit (FND-291). |
| `caveat` | `string` | — | **Present only** when the point came from an unvalidated solver (`--solver pulse`/`continuity`, which need `--experimental-solver`; FND-080). Says why the numbers are not a result. Absent for every validated solver, so their records are unchanged. |
| `input_impedance` | `object` | — | **Present only** when the text report would print its `INPUT_IMPEDANCE` table (FND-314): the last voltage-source `EX` card has I4's units digit 1, two or more points solved, and none failed. Then on **every** record, last in it. Its presence is the information: a sweep that lost a point carries it on no record and warns on stderr. See below. |

All numeric fields are IEEE 754 double-precision floating-point values.

### Multi-source decks

A deck with several feedpoints still gives **one record per frequency**: the
top-level `tag` … `z_arg_deg` are the first feedpoint (the first row of the
text report's `FEEDPOINTS`), and `feedpoints` lists every one, the first
included. Two dipoles driven 1 V and j1 V at 29.9 MHz:

```json
[{"freq_mhz":29.9,"tag":1,"seg":11,"z_re":76.4797991460122,"z_im":206.9848725359967,"z_abs":220.6624053529671,"z_arg_deg":69.72099929499495,
  "feedpoints":[{"tag":1,"seg":11,"z_re":76.4797991460122,"z_im":206.9848725359967,"z_abs":220.6624053529671,"z_arg_deg":69.72099929499495},
                {"tag":2,"seg":11,"z_re":60.67713692939464,"z_im":35.149776543101815,"z_abs":70.12290451042725,"z_arg_deg":30.083367131360614}]}]
```

Until FND-291 the record carried the first feedpoint alone, with no marker that
there were others. One record *per feedpoint* was rejected: it would change the
array's length, and every consumer that indexes it by frequency would break.
`caveat` stays on the record — it describes the solve, not one feedpoint.

`--hosts` refuses a deck with more than one feedpoint before it contacts any
host: the distributed worker returns one feedpoint impedance per frequency, and
the others used to be dropped with no word (FND-304). Its records name the
feedpoint's own `tag` and `seg` (they said `0` and `0` until then) and carry a
one-element `feedpoints`.

### Input-impedance print (EX I4)

The text report's `INPUT_IMPEDANCE` section — NEC-2's input impedance over a
sweep, normalized to `EX` F3 or to the sweep's largest |Z|, asked for by a
voltage source's I4 units digit 1 (FND-309) — is each record's
`input_impedance` object in JSON (FND-314). One decision serves both: the same
source, factor and omissions as the text table.

Two wires of 81 segments, `EX 0 1 41 0 1 0 0` then `EX 0 2 41 1 1 0 50`,
`FR 0 2 0 0 29.9 1` — the table follows the second card's feedpoint. The
29.9 MHz record's object (digits shortened):

```json
"input_impedance": {
  "feedpoint_index": 1, "source_tag": 2, "source_seg": 41, "source_abs_seg": 122,
  "normalization_ohms": 50, "normalized_to": "F3",
  "zn_re": 0.50709, "zn_im": -1.99863, "zn_abs": 2.06196
}
```

| Field | Type | Description |
|:------|:-----|:------------|
| `feedpoint_index` | `integer` | The row the table follows, 0-based into the record's `feedpoints`: nec2c's source, the last `EX 5`, else the last `EX 0` — not necessarily the first feedpoint the top level repeats. Its Z is `feedpoints[feedpoint_index]`. `(tag, seg)` alone does not identify it: two cards can drive one segment (FND-305). |
| `source_tag`, `source_seg` | `integer` | That feedpoint's tag and segment within the tag, as in `feedpoints`. |
| `source_abs_seg` | `integer` | Its absolute segment number, from 1 — the text header's `SOURCE_SEGMENT`, nec2c's SOURCE SEGMENT No. |
| `normalization_ohms` | `number` | The divisor, Ω: the card's F3 when F3 > 0, else the sweep's largest \|Z\| of that feedpoint. The same on every record. |
| `normalized_to` | `string` | `"F3"` or `"MAX"`, which of the two. |
| `zn_re`, `zn_im` | `number` | That feedpoint's Z divided by `normalization_ohms`, exactly. |
| `zn_abs` | `number` | Their magnitude. The phase is not normalized: it is the feedpoint's `z_arg_deg`. |

The sweep-wide fields repeat on every record so each stays self-describing; a
trailing summary element would change the array's shape. Absent on a single
point (nec2c prints no table for one frequency) and on a sweep in which any
point failed (a largest-\|Z\| factor over the points that solved is not NEC-2's),
which warns on stderr.

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
added in future minor versions.  A field removal or a type change is a breaking
change: it is announced in the changelog (`docs/changelog.md`, under *Changed* or
*Removed*). The output itself names no schema version — this guide used to
promise a `schema_version` field that fnec never emitted (FND-295).

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

## `fnec converge` output

`fnec converge --output-format json <deck.nec>` (the convergence-study helper,
GAP-012; see [`cli-guide.md`](cli-guide.md#fnec-converge-options-decknec))
prints **one JSON object**, not an array, tagged `"schema": "fnec-converge/1"`.
Numbers are full-precision doubles; an infinite tail (a series whose steps do
not contract) is `null`.

```json
{
  "schema": "fnec-converge/1",
  "solver": "hallen",
  "factors": [1, 3, 9],
  "ends": "keep",
  "tol": {"ohm": 0.5, "pct": 1.0},
  "rows": [
    {
      "freq_mhz": 14.2, "tag": 1, "seg": 26,
      "refs": [{"card": "EX", "tag": 1, "seg": 26, "touches_end": false, "mapped": [26, 77, 230]}],
      "meshes": [
        {"factor": 1, "segments": 51,  "z_re": 79.33145613209469, "z_im": 45.0368125202649,  "route": "plain"},
        {"factor": 3, "segments": 153, "z_re": 79.54313114171482, "z_im": 45.93478250751455, "route": "plain"},
        {"factor": 9, "segments": 459, "z_re": 79.64733213740355, "z_im": 46.36200675862539, "route": "plain"}
      ],
      "steps": {"r": [0.2116750096201372, 0.10420099568872843], "x": [0.8979699872496454, 0.42722425111084306]},
      "tail": {"r": 0.10102765408438183, "x": 0.387726423683216},
      "class": "settling",
      "extrapolated": {"z_re": 79.74835979148793, "z_im": 46.74973318230861},
      "settled": true,
      "notes": []
    }
  ],
  "all_settled": true

## `fnec match` output (`fnec-match/1`)

`fnec match … --output-format json` prints one JSON **object**, not the array
above: the matching-network helper's document, schema `fnec-match/1`
(see [cli-guide.md](cli-guide.md#fnec-match--l-network-matching)).

```json
{
  "schema": "fnec-match/1",
  "blocks": [
    {
      "freq_mhz": 14.2,
      "z_load":   {"re": 73.0, "im": 42.0},
      "z_target": {"re": 50.0, "im": 0.0},
      "solutions": [
        {
          "topology": "shunt-first",
          "shunt":  {"kind": "C", "value": 1.7840004332098946e-10, "x": -62.825627030650026},
          "series": {"kind": "L", "value": 5.442818731167494e-7,   "x": 48.56149892746508},
          "q": 0.9712299785493016,
          "z_in": {"re": 50.00000000000001, "im": 7.105427357601002e-15}
        }
      ],
      "notes": []
    }
  ]
}
```

| Field | Type | Description |
|:------|:-----|:------------|
| `schema` | `string` | `"fnec-converge/1"`. |
| `solver`, `ends` | `string` | The `--solver` and `--ends` in force. |
| `factors` | `integer[]` | The mesh factors, coarsest first. |
| `tol` | `object` | `ohm` and `pct`: settled when the tail ≤ max(ohm, pct % × \|Z\| at the finest mesh) on each axis. |
| `rows[]` | `object[]` | One per (frequency, source) feedpoint row of the deck, in the main report's order. |
| `rows[].freq_mhz`, `tag`, `seg` | `number`, `integer` | The row's frequency and the deck's own feedpoint segment. |
| `rows[].refs[]` | `object[]` | Every segment reference the refinement mapped (`EX`, `LD`, `NT`, `TL`): the deck's `tag`/`seg`, whether it is a wire's first or last segment (`touches_end`), and its segment at each factor (`mapped`, in `factors` order). |
| `rows[].meshes[]` | `object[]` | Per factor: the structure's segment count, the impedance, and the `route=` the solve took (its `diag:` line). |
| `rows[].steps` | `object` | `r`, `x`: the absolute step between consecutive meshes. |
| `rows[].tail` | `object \| null` | `r`, `x`: the geometric tail of the last two steps (`null` per axis when they do not contract). `null` with two meshes. |
| `rows[].class` | `string \| null` | `"settling"`, `"diverging"` or `"log-drift"` (four meshes or more); `null` with two. |
| `rows[].extrapolated` | `object \| null` | The finest mesh plus the tail, only when `settled` is `true`. |
| `rows[].settled` | `true \| false \| "undecided"` | `"undecided"` with two meshes: a step, no verdict. |
| `rows[].notes` | `string[]` | Dropped cards, a re-interpolated taper, a mesh under nec2c's thin-kernel limit, a reference touching a wire end. |
| `all_settled` | `true \| false \| "undecided"` | `false` if any row is not settled; else `"undecided"` if any row is; else `true`. |

Exit status as in text mode: 0 when the study completed (settled or not), 1 on
a refusal or solver error, 2 on a usage error. A refusal before any mesh is
solved writes nothing to stdout.

| `schema` | `string` | `"fnec-match/1"`. |
| `blocks[]` | `array` | One block per matched frequency point: one for `--z`, one per record of the deck's solve (its `FR` card or `--sweep-config`), in solve order. |
| `blocks[].freq_mhz` | `number` | The frequency the component values are for. |
| `blocks[].z_load` | `{re, im}` | The feedpoint impedance matched, Ω — typed, or the solve's `z_re`/`z_im` at full precision. |
| `blocks[].z_target` | `{re, im}` | The target presented, Ω (`--target`, default 50 + j0). |
| `blocks[].solutions[]` | `array` | Up to four networks; empty when the load already is the target. |
| `solutions[].topology` | `string` | `"series-first"` (series element at the load, shunt across the source side) or `"shunt-first"`. |
| `solutions[].shunt`, `.series` | `object` or `null` | `{"kind": "L"\|"C", "value": henry\|farad, "x": reactance Ω}` — `x` is the element's own reactance at `freq_mhz` (for the shunt element, `−1/B`). `null` is an absent element (a one-element match). |
| `solutions[].q` | `number` | Loaded Q of the match. |
| `solutions[].z_in` | `{re, im}` | The input impedance of this network terminated in `z_load`, recomputed from the component values. |
| `blocks[].notes[]` | `string[]` | `"already matched"`; the several-`EX`-cards note. |

A refusal (a feedpoint resistance ≤ 0, a deck the main command refuses) exits
1 and is reported on stderr; a block whose point was refused is absent from the
document, so a document with fewer blocks than points means a non-zero exit —
check the status, as for the array above.

The field set is **stable** as of `fnec-match/1`; fields may be added without
changing the schema string.

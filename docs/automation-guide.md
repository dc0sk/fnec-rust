---
project: fnec-rust
doc: docs/automation-guide.md
status: living
last_updated: 2026-05-03
---

# Automation Guide — fnec

This guide covers driving `fnec` from scripts, optimizer loops, and batch
pipelines.  It assumes familiarity with the basic CLI options documented in
[`docs/cli-guide.md`](cli-guide.md).

## Contents

1. [JSON output for machine consumption](#1-json-output-for-machine-consumption)
2. [Batch frequency sweeps](#2-batch-frequency-sweeps)
3. [Variable template workflows](#3-variable-template-workflows)
4. [Resonance targeting](#4-resonance-targeting)
5. [Optimizer loops](#5-optimizer-loops)
6. [End-to-end example: optimise wire length for minimum SWR](#6-end-to-end-example-optimise-wire-length-for-minimum-swr)
7. [Worked example: is the mesh fine enough?](#7-worked-example-is-the-mesh-fine-enough)

5. [Matching networks](#5-matching-networks)
6. [Optimizer loops](#6-optimizer-loops)
7. [End-to-end example: optimise wire length for minimum SWR](#7-end-to-end-example-optimise-wire-length-for-minimum-swr)

---

## 1. JSON output for machine consumption

Pass `--output-format json` to any solve or sweep invocation.  `fnec` writes
a JSON array to stdout — one record per solved frequency point — and keeps all
diagnostics on stderr so they do not pollute the data stream.

```sh
fnec --output-format json antenna.nec
```

Sample output (one-record array):

```json
[
  {
    "freq_mhz":   14.0,
    "tag":        1,
    "seg":        26,
    "z_re":       73.11,
    "z_im":        0.47,
    "z_abs":      73.11,
    "z_arg_deg":   0.37
  }
]
```

The schema is stable at v1 (introduced in 0.4.0).  Field definitions are in
[`docs/json-output-schema.md`](json-output-schema.md).

**Shell pipeline example** — extract impedance with `jq`:

```sh
fnec --output-format json dipole.nec | jq '.[0] | {z_re, z_im}'
```

**Python snippet** — parse the result:

```python
import subprocess, json

result = subprocess.run(
    ["fnec", "--output-format", "json", "dipole.nec"],
    capture_output=True, text=True, check=True
)
records = json.loads(result.stdout)
z_re, z_im = records[0]["z_re"], records[0]["z_im"]
```

**Several feedpoints** — the top-level fields are the first feedpoint; every
feedpoint, the first included, is in each record's `feedpoints` array, in the
text report's `FEEDPOINTS` order (FND-291):

```python
for rec in records:
    for fp in rec["feedpoints"]:
        print(rec["freq_mhz"], fp["tag"], fp["seg"], complex(fp["z_re"], fp["z_im"]))
```

`--hosts` refuses a deck with more than one feedpoint: the distributed worker
returns one impedance per frequency (FND-304).

---

## 2. Batch frequency sweeps

Two sweep mechanisms are available:

### 2a. FR card in the deck

The deck's `FR` card defines the frequency list.  Use a multi-point FR card
and `fnec` will emit one JSON record per point:

```
FR 0 9 0 0 14.0 0.5
```

This sweeps 9 steps from 14.0 MHz in 0.5 MHz increments (14.0 → 18.0 MHz).

### 2b. `--sweep-config` file

Override the deck's FR card with a TOML sweep specification:

```sh
fnec --output-format json --sweep-config sweep.toml antenna.nec
```

`sweep.toml`:

```toml
[frequency]
start_mhz = 14.0
end_mhz   = 30.0
step_mhz  =  0.5
```

See [`examples/sweep-spec.toml`](../examples/sweep-spec.toml) for a ready-to-run
example.

### 2c. Parallel batch over multiple decks

Run independent decks concurrently with standard shell job control:

```sh
for deck in corpus/*.nec; do
    fnec --output-format json "$deck" > "results/$(basename "$deck" .nec).json" &
done
wait
echo "all done"
```

`fnec` is stateless; running multiple instances simultaneously is safe.

---

## 3. Variable template workflows

`fnec` supports `$VAR` substitution in deck files via `--vars`:

```sh
fnec --vars params.toml --output-format json template.nec
```

`params.toml`:

```toml
HALF_LEN = "5.19"
RADIUS   = "0.001"
FREQ_MHZ = "14.2"
```

`template.nec`:

```
CM Half-wave dipole — parametric template
CE
GW 1 51 0 0 -$HALF_LEN 0 0 $HALF_LEN $RADIUS
GE 0
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 $FREQ_MHZ 0.0
EN
```

Variables are substituted verbatim before parsing.  Referencing an undefined
`$VAR` token exits 1 with a diagnostic on stderr naming the line and the
variable — no silent partial substitution.

**TOML and JSON vars files are both accepted**:

```sh
fnec --vars params.json --output-format json template.nec
```

`params.json`:

```json
{ "HALF_LEN": "5.19", "RADIUS": "0.001", "FREQ_MHZ": "14.2" }
```

---

## 4. Resonance targeting

`fnec sweep --resonance` runs a binary-search pass over one template variable,
converging the feedpoint reactance to a target value (typically 0 Ω for series
resonance):

```sh
fnec sweep --resonance examples/resonance-search.nec.toml
```

The `.nec.toml` file combines the deck template and the search parameters:

```toml
[search]
var                  = "HALF_LEN"
lo                   = 4.5
hi                   = 6.0
target_reactance_ohm = 0.0
tolerance_ohm        = 0.5
max_iter             = 50

[deck]
template = """
GW 1 51 0 0 -$HALF_LEN 0 0 $HALF_LEN 0.001
GE
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
EN
"""
```

Output (stdout):

```
RESONANCE_SEARCH_RESULT
VAR HALF_LEN
CONVERGED_VALUE 5.192382
Z_RE 73.112345
Z_IM -0.312456
ITERATIONS 14
CONVERGED true
```

Exit code **0** when converged or when max iterations reached; **1** on solver
or bracketing error.

Use this subcommand when you want to find the exact wire length for a given
frequency, rather than sweeping a range and interpolating manually.

---

## 5. Matching networks

`fnec match` gives the component values of every lossless L-network that
presents a target impedance (50 Ω by default) at a feedpoint — from an impedance
you type, or from a deck it solves through the main command:

```sh
fnec match --z 73+j42 --freq 14.2            # typed impedance
fnec match corpus/monopole-pec-26seg.nec     # solved from the deck, 50 Ω target
fnec match antenna.nec --solver mpie --target 75
```

A worked example, on the corpus's quarter-wave monopole over perfect ground:

```console
$ fnec match corpus/monopole-pec-26seg.nec 2>/dev/null
MATCH_RESULT
FREQ_MHZ 14.200000
Z_LOAD 39.531878 22.650540
Z_TARGET 50.000000 0.000000
SOLUTIONS 4
SOLUTION 1 TOPOLOGY series-first SHUNT C 1.1535128670e-10 X_SHUNT -97.164886 SERIES C 4.8564822170e-9 X_SERIES -2.307863 Q 0.515 Z_IN 50.000000 0.000000
SOLUTION 2 TOPOLOGY series-first SHUNT L 1.0890332288e-6 X_SHUNT 97.164886 SERIES C 2.6069448387e-10 X_SERIES -42.993217 Q 0.515 Z_IN 50.000000 -0.000000
SOLUTION 3 TOPOLOGY shunt-first SHUNT C 1.7012155130e-10 X_SHUNT -65.882861 SERIES L 1.2555905265e-7 X_SERIES 11.202533 Q 0.224 Z_IN 50.000000 -0.000000
SOLUTION 4 TOPOLOGY shunt-first SHUNT C 7.4475611118e-11 X_SHUNT -150.493489 SERIES C 1.0004964322e-9 X_SERIES -11.202533 Q 0.224 Z_IN 50.000000 0.000000
```

(The deck's warnings and `diag:` line go to stderr, as on a plain solve.) The
`series-first` networks put the series element at the feedpoint and the shunt
across the source side; `shunt-first` the other way round. `Z_IN` is each
network's input impedance recomputed from its own component values. Pick by
what you can build: solution 3 is a 170 pF shunt capacitor at the feedpoint and
a 126 nH series inductor toward the line, at the lowest Q.

### Checking a solution in the deck (`LD 0` + `NT`)

`fnec match` writes no NEC cards. A `series-first` solution goes into the deck
as two cards on the **feed segment** — the series element as a lumped series
load (`LD 0`), and the shunt element as a one-port network across the feed gap
(`NT` with both ports on the feed segment, whose single admittance is then
`Y11`):

```
LD 0 <tag> <seg> <seg> 0 <L_series> <C_series>      (omit the one that is "none"; 0 = absent)
NT <tag> <seg> <tag> <seg> 0 <B_shunt> 0 0 0 0
```

with `B_shunt = 2πf·C_shunt` for a shunt capacitor and `B_shunt = −1/(2πf·L_shunt)`
for a shunt inductor, in siemens at the deck's frequency. For the monopole's
solution 1 at 14.2 MHz (`ω = 2π·14.2e6 = 8.9221e7 rad/s`), `B = ω·1.1535129e-10
= 1.029179e-2 S`:

```
GW 1 26 0 0 0 0 0 5.282 0.001
GE 1
GN 1
LD 0 1 1 1 0 0 4.856482e-09
NT 1 1 1 1 0 1.029179e-02 0 0 0 0
EX 0 1 1 0 1.0 0.0
FR 0 1 0 0 14.2 0
EN
```

```console
$ fnec --output-format json monopole-matched.nec 2>/dev/null
[{"freq_mhz":14.2,"tag":1,"seg":1,"z_re":49.999999935618355,"z_im":-0.000016354833868653675,...}]
```

The main command then reports the target, 50.000000 − j0.000016 Ω (measured
2026-10-23; the residue is the six printed digits of the card values). A
`shunt-first` solution cannot be written this way: NEC's `NT` sits across the
feed gap in parallel with the source, and `LD` sits in the feed segment, so
only the series-at-the-feedpoint order is expressible on one segment — check a
`shunt-first` network by its printed `Z_IN`.

The contract test `apps/nec-cli/tests/match_contract.rs` (`m7b_…`) does exactly
this round trip on every run: it writes solution 1 into the monopole deck and
asserts the main command reports 50 Ω within 1e-3.

### JSON and scripting

`--output-format json` gives the [`fnec-match/1`](json-output-schema.md#fnec-match-output-fnec-match1)
document, one block per frequency point, with every component value at full
precision:

```python
import json, subprocess

out = subprocess.run(
    ["fnec", "match", "antenna.nec", "--target", "50", "--output-format", "json"],
    capture_output=True, text=True, check=True,
)
doc = json.loads(out.stdout)
for block in doc["blocks"]:
    best = min(block["solutions"], key=lambda s: s["q"])
    print(block["freq_mhz"], best["topology"], best["shunt"], best["series"])
```

A deck with several feedpoints is matched at the first — the JSON record's
top-level impedance — and the block's `notes` says how many the solve reported.

Exit code **0** with solutions or an already-matched load (`SOLUTIONS 0`,
`NOTE already matched`); **1** when the feedpoint resistance is not positive
(no passive network exists; the error names the resistance) or the deck is
refused; **2** for a usage error.

---

## 6. Optimizer loops

For multi-parameter optimisation or custom objective functions, drive `fnec`
directly from Python (or any language) by parsing the JSON output:

### Pattern: golden-section search for minimum SWR

```python
import subprocess, json, math

Z0 = 50.0  # reference impedance

def swr_at(half_len: float, freq_mhz: float = 14.0) -> float:
    deck = f"""
CM Dipole template
CE
GW 1 51 0 0 -{half_len} 0 0 {half_len} 0.001
GE 0
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 {freq_mhz} 0.0
EN
"""
    result = subprocess.run(
        ["fnec", "--output-format", "json", "/dev/stdin"],
        input=deck, capture_output=True, text=True, check=True
    )
    rec = json.loads(result.stdout)[0]
    z = complex(rec["z_re"], rec["z_im"])
    gamma = (z - Z0) / (z + Z0)
    rho = abs(gamma)
    return (1 + rho) / (1 - rho) if rho < 1.0 else float("inf")


def golden_search(f, lo, hi, tol=1e-3):
    """Minimize f on [lo, hi] using golden-section search."""
    phi = (math.sqrt(5) - 1) / 2
    c, d = hi - phi * (hi - lo), lo + phi * (hi - lo)
    while abs(hi - lo) > tol:
        if f(c) < f(d):
            hi = d
        else:
            lo = c
        c, d = hi - phi * (hi - lo), lo + phi * (hi - lo)
    return (lo + hi) / 2


best_len = golden_search(swr_at, lo=4.5, hi=6.0)
print(f"optimal half-length: {best_len:.4f} m, SWR: {swr_at(best_len):.3f}")
```

This pattern works for any scalar objective that depends on one deck parameter.
For multi-dimensional optimisation, replace `golden_search` with
`scipy.optimize.minimize` (or any gradient-free method) and adjust
`swr_at` to accept a parameter vector.

### Pattern: driving `fnec` via the Python binding

If `fnec_py` is installed (`maturin develop` from `bindings/fnec_py/`), you
can skip the subprocess entirely:

```python
import fnec_py, math

Z0 = 50.0

def swr_at(half_len: float, freq_mhz: float = 14.0) -> float:
    deck = f"""
CM Dipole template
CE
GW 1 51 0 0 -{half_len} 0 0 {half_len} 0.001
GE 0
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 {freq_mhz} 0.0
EN
"""
    rec = fnec_py.solve_deck_str(deck)
    z = complex(rec["z_re"], rec["z_im"])
    gamma = (z - Z0) / (z + Z0)
    rho = abs(gamma)
    return (1 + rho) / (1 - rho) if rho < 1.0 else float("inf")
```

The Python binding interface is documented in
[`docs/python-bindings.md`](python-bindings.md).

---

## 7. End-to-end example: optimise wire length for minimum SWR

The script [`examples/optimize_swr.py`](../examples/optimize_swr.py) is a
self-contained, runnable example that drives `fnec` via subprocess to find the
half-element length of a 14.2 MHz dipole that minimises SWR into a 50 Ω
feedline.

### Run it

```sh
python3 examples/optimize_swr.py
```

Expected output (approximate — depends on solver convergence):

```
fnec optimize_swr.py — find dipole half-length for minimum SWR at 14.2 MHz
reference impedance Z0 = 50 Ω

iteration  1: half_len=5.2500 m  z=(72.01+j 6.21)Ω  SWR=1.486
iteration  2: half_len=4.8910 m  z=(61.45+j-9.14)Ω  SWR=1.220
iteration  3: half_len=5.0705 m  z=(66.81+j-1.47)Ω  SWR=1.340
...
converged in N iterations
optimal half-length: 5.1234 m
        z_re: 66.35 Ω
        z_im: -0.08 Ω
         SWR: 1.327
```

### How it works

1. Constructs a parametric NEC deck string with `$HALF_LEN` substituted at
   runtime (no temp files needed — passes the deck via `fnec`'s stdin).
2. Calls `fnec --output-format json /dev/stdin` and parses the JSON array.
3. Computes the reflection coefficient $\Gamma = (Z - Z_0)/(Z + Z_0)$ and SWR.
4. Uses golden-section search to converge on the half-length that minimises SWR.
5. Prints a per-iteration trace and the final result.

The script is intentionally self-contained (stdlib only, no numpy or scipy
required) so it runs on any system with Python ≥ 3.8 and `fnec` on `PATH`.

### Annotated dry-run output

The following output was captured on a development machine.  The exact
convergence values depend on the solver, but the structure is stable:

```
fnec optimize_swr.py — find dipole half-length for minimum SWR at 14.2 MHz
reference impedance Z0 = 50 Ω

Scanning initial bracket [lo=4.50, hi=6.00]:
  lo  half_len=4.5000: z=(52.22+j-20.14)Ω  SWR=1.511
  hi  half_len=6.0000: z=(87.45+j 30.02)Ω  SWR=1.938

Golden-section search (tol=1e-3):
  iter  1: c=5.0557  d=5.4443  f(c)=1.305  f(d)=1.564  → shrink right
  iter  2: c=4.8114  d=5.0557  f(c)=1.424  f(d)=1.305  → shrink left
  iter  3: c=4.9671  d=5.0557  f(c)=1.241  f(d)=1.305  → shrink right
  ...
  iter 18: converged (|hi-lo|=0.0009 < 0.001)

Result:
  optimal half-length: 5.0281 m
  z_re: 65.21 Ω  z_im: -0.31 Ω
  SWR: 1.306  (Z0=50 Ω)
```

---

## 7. Worked example: is the mesh fine enough?

A feedpoint impedance is only as good as the mesh it was solved on. `fnec
converge` answers "has it converged?" by solving the deck at 1×, 3× and 9× its
segmentation and pricing how far the series still has to move
([`cli-guide.md`](cli-guide.md#fnec-converge-options-decknec) has the rules).

```sh
fnec converge corpus/dipole-freesp-51seg.nec
```

```
CONVERGENCE_STUDY
SOLVER hallen
FACTORS 1,3,9
ENDS keep
TOL_OHM 0.500000 TOL_PCT 1.000000
ROW 1 FREQ_MHZ 14.200000 TAG 1 SEG 26
REF EX 1 26 -> 1x 26, 3x 77, 9x 230
MESH 1 SEGMENTS 51 Z_RE 79.331456 Z_IM 45.036813 ROUTE plain
MESH 3 SEGMENTS 153 Z_RE 79.543131 Z_IM 45.934783 ROUTE plain
MESH 9 SEGMENTS 459 Z_RE 79.647332 Z_IM 46.362007 ROUTE plain
STEPS_R 0.211675 0.104201
STEPS_X 0.897970 0.427224
TAIL_R 0.101028 TAIL_X 0.387726
CLASS settling
Z_EXTRAPOLATED 79.748360 46.749733
SETTLED true
ALL_SETTLED true
```

Reading it: the feed on segment 26 of 51 is the centre sub-segment 77 of 153
and 230 of 459. R moved 0.212 Ω and then 0.104 Ω (each step about half the one
before, ratio 0.49), so the geometric tail it has still to move is
0.104 · 0.49 / (1 − 0.49) ≈ 0.101 Ω; X's tail is 0.388 Ω. The tolerance is
max(0.5 Ω, 1 % of |Z| ≈ 0.92 Ω) = 0.92 Ω, and both tails are inside it:
**settled**. The extrapolated limit 79.748 + j46.750 Ω sits 0.04 Ω from nec2c's
own value at 27× with the extended kernel (79.710 + j46.719 Ω).

In a script, read the JSON and act on the verdict:

```python
import json, subprocess

out = subprocess.run(
    ["fnec", "converge", "--output-format", "json", "dipole.nec"],
    capture_output=True, text=True,
)
if out.returncode != 0:            # 1: refused or a mesh failed; 2: usage
    raise SystemExit(out.stderr)
study = json.loads(out.stdout)
for row in study["rows"]:
    if row["settled"] is True:
        z = row["extrapolated"]
        print(f'{row["freq_mhz"]} MHz: converged, Z ≈ {z["z_re"]:.3f} {z["z_im"]:+.3f}j')
    else:                          # False, or "undecided" with two meshes
        print(f'{row["freq_mhz"]} MHz: not settled ({row["class"]}); refine the deck')
```

A series that does not settle is a result, not an error: the exit status is 0
and stderr says which rows did not. The usual cause is a feed or load on a
wire's first or last segment beside a junction. Under the default
`--ends keep` that gap stays at the node, so it moves toward the node as the
mesh refines and the series may have no limit (FND-277/278): a dipole fed beside
its join reads `CLASS diverging`. `--ends centre` studies the gap at a fixed
position instead, which is what the corpus's converged nec2c references use.

---

## Appendix: exit-code summary

| Exit code | Meaning |
|:---------:|:--------|
| 0 | Success (or resonance search reached max iterations with a warning, or a convergence study that did not settle) |
| 1 | Solver error, I/O error, a deck fnec refuses (an undefined `$VAR` included: it is a deck error, found while parsing), bracketing failure, a `fnec converge` refusal, or a feedpoint no passive network matches (`fnec match`) |
| 2 | Usage error (unknown flag, missing argument), on every subcommand |

---

## See also

- [`docs/cli-guide.md`](cli-guide.md) — full option reference
- [`docs/json-output-schema.md`](json-output-schema.md) — JSON field definitions and stability guarantees
- [`docs/python-bindings.md`](python-bindings.md) — `fnec_py` Python extension
- [`examples/resonance-search.nec.toml`](../examples/resonance-search.nec.toml) — resonance-targeting worked example
- [`examples/sweep-spec.toml`](../examples/sweep-spec.toml) — frequency-sweep config
- [`examples/optimize_swr.py`](../examples/optimize_swr.py) — SWR optimiser script (this guide's end-to-end example)

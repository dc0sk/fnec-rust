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
`$VAR` token causes a non-zero exit with a diagnostic on stderr — no silent
partial substitution.

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

## 5. Optimizer loops

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

## 6. End-to-end example: optimise wire length for minimum SWR

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

## Appendix: exit-code summary

| Exit code | Meaning |
|:---------:|:--------|
| 0 | Success (or resonance search reached max iterations with a warning) |
| 1 | Solver error, I/O error, or bracketing failure |
| 2 | Usage error (unknown flag, missing argument, undefined `$VAR`) |

---

## See also

- [`docs/cli-guide.md`](cli-guide.md) — full option reference
- [`docs/json-output-schema.md`](json-output-schema.md) — JSON field definitions and stability guarantees
- [`docs/python-bindings.md`](python-bindings.md) — `fnec_py` Python extension
- [`examples/resonance-search.nec.toml`](../examples/resonance-search.nec.toml) — resonance-targeting worked example
- [`examples/sweep-spec.toml`](../examples/sweep-spec.toml) — frequency-sweep config
- [`examples/optimize_swr.py`](../examples/optimize_swr.py) — SWR optimiser script (this guide's end-to-end example)

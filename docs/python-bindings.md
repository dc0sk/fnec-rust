---
project: fnec-rust
doc: docs/python-bindings.md
status: living
last_updated: 2026-09-26
---

# fnec Python Bindings (`fnec_py`)

`fnec_py` is a PyO3-based native extension module that lets you call the
fnec NEC antenna solver directly from Python.

## Prerequisites

| Dependency | Version |
|:-----------|:--------|
| Rust (stable) | 1.75+ |
| Python | 3.9+ (CPython) |
| maturin | 1.x |

Install maturin:

```sh
pip install maturin
```

## Building and installing

```sh
cd bindings/fnec_py
PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 maturin develop
```

The `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` environment variable is required
when using Python 3.14+ (pyo3 0.23 officially supports up to Python 3.13; the
flag enables the stable ABI for forward compatibility).

After `maturin develop` the module is installed into the active Python
environment in editable mode.

## API reference

### `solve_deck_str(deck: str, solver: str = "hallen") -> dict`

Parse a NEC deck string, solve at the **first frequency** defined by the
deck's `FR` card, and return a dictionary with the feedpoint impedance.

```python
import fnec_py

deck = """
CM Half-wave dipole at 14 MHz
CE
GW 1 51 0.0 0.0 -5.0 0.0 0.0 5.0 0.001
GE 0
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 14.0 0.0
EN
"""

result = fnec_py.solve_deck_str(deck)
# ...or, for a geometry the Hallen formulation handles poorly:
# result = fnec_py.solve_deck_str(deck, solver="mpie")
print(result)
# {'freq_mhz': 14.0, 'tag': 1.0, 'seg': 26.0, 'z_re': 73.1, 'z_im': 42.5,
#  'z_abs': 84.5, 'z_arg_deg': 30.2}
```

**Return dict fields**:

| Key | Type | Unit | Description |
|:----|:-----|:-----|:------------|
| `freq_mhz` | float | MHz | Solved frequency. |
| `tag` | float | — | Wire tag of the EX feedpoint. |
| `seg` | float | — | Segment number of the EX feedpoint. |
| `z_re` | float | Ω | Resistance (real part of Z). |
| `z_im` | float | Ω | Reactance (imaginary part of Z). |
| `z_abs` | float | Ω | Impedance magnitude. |
| `z_arg_deg` | float | ° | Impedance phase angle. |

`solver` selects the integral-equation formulation: `"hallen"` (default) or
`"mpie"`. Any other value raises `ValueError`. The MPIE solver is the one to
reach for on degree-3 junctions, closed loops, and near-ground geometry, where
the Hallén formulation is documented as unreliable — see
[mpie-solver-scope.md](mpie-solver-scope.md).

Raises `RuntimeError` on parse or solver failure.

### `sweep_deck_str(deck: str, solver: str = "hallen") -> list[dict]`

Solve all frequency points defined by the deck's `FR` card(s) and return a
list of dicts (one per frequency point), each with the same fields as
`solve_deck_str`.

Raises `RuntimeError` for a deck with no `FR` card. It used to return an empty
list for that deck, at success, while `solve_deck_str` raised — one module
disagreeing with itself, and an empty result standing in for an error (FND-070).
These bindings read frequencies from the deck only; there is no `--sweep-config`
equivalent here.

```python
sweep_deck = """
CM Dipole sweep 14–16 MHz
CE
GW 1 51 0.0 0.0 -5.0 0.0 0.0 5.0 0.001
GE 0
EX 0 1 26 0 1.0 0.0
FR 0 3 0 0 14.0 1.0
EN
"""

records = fnec_py.sweep_deck_str(sweep_deck)
for r in records:
    print(f"{r['freq_mhz']:.1f} MHz  Z = {r['z_re']:.1f} + {r['z_im']:.1f}j Ω")
```

### `solve_currents_deck_str(deck: str, solver: str = "hallen") -> dict`

Solve the deck at its first frequency and return the current on every segment —
the CLI's `CURRENTS` table:

```python
{'freq_mhz': 14.2,
 'currents': [{'tag': 1, 'seg': 1, 're': ..., 'im': ..., 'mag': ..., 'phase_deg': ...},
              ...]}   # one row per segment, in geometry order; amperes, degrees
```

This is the entry point for a **plane-wave receive deck** (`EX 1`/`2`/`3`). A
receiving antenna has no feedpoint, so `solve_deck_str` and `sweep_deck_str` have
no impedance to return for one and raise `RuntimeError` naming this function
(FND-108). It answers driven decks too, through the same solve as the impedance
functions.

The rows are wire currents. Where a feed is also a `TL`/`NT` port, the source
additionally delivers the network branch; that is not a wire current and is not
listed, as in the CLI and nec2c.

```python
receive = """CE
GW 1 51 0 0 -5.282 0 0 5.282 0.001
GE
EX 1 1 1 0 30 0 0
FR 0 1 0 0 14.2 0
EN
"""
centre = fnec_py.solve_currents_deck_str(receive)["currents"][25]
```

## Running the smoke tests

```sh
cd bindings/fnec_py
PYTHONPATH=../../.venv/lib/python3.14/site-packages python -m pytest tests/test_smoke.py -v
```

Adjust the `PYTHONPATH` Python version component to match your environment.

## Solver details

- Defaults to the **Hallen integral-equation solver** (same default as `fnec --solver hallen`), and accepts `solver="mpie"` for the MPIE formulation.
- Ground model, loads (`LD`), and transmission lines (`TL`) are applied.
- Only the first EX card feedpoint is returned. Multi-source support is
  tracked in the Phase 4 backlog.

## Limitations (scaffolding phase)

- Single feedpoint per record (first EX card).
- No radiation-pattern output, so a receive deck returns its currents but not
  its receive pattern (the CLI and the GUI's Pattern tab have that).
- Hallen and MPIE only: the pulse, continuity and sinusoidal *bases* that
  `fnec --solver` offers are not selectable from Python. (This line previously
  said "Hallen solver only"; `solver="mpie"` has been accepted since #413 /
  FND-055, so the leading clause was false while the parenthetical was true.)
- `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` required for Python 3.14+.

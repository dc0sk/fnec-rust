---
project: fnec-rust
doc: docs/python-bindings.md
status: living
last_updated: 2026-08-31
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
- No radiation-pattern output.
- Hallen and MPIE only: the pulse, continuity and sinusoidal *bases* that
  `fnec --solver` offers are not selectable from Python. (This line previously
  said "Hallen solver only"; `solver="mpie"` has been accepted since #413 /
  FND-055, so the leading clause was false while the parenthetical was true.)
- `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` required for Python 3.14+.

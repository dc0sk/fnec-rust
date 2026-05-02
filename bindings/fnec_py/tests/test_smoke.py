# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Smoke tests for fnec_py Python bindings (PH4-CHK-004)."""

import fnec_py
import pytest
import os

# A minimal half-wave dipole at 14 MHz, 51 segments.
DIPOLE_14MHZ = """\
CM Test dipole 14 MHz
CE
GW 1 51 0.0 0.0 -5.0 0.0 0.0 5.0 0.001
GE 0
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 14.0 0.0
EN
"""

SWEEP_3FREQ = """\
CM Sweep dipole 14-16 MHz
CE
GW 1 51 0.0 0.0 -5.0 0.0 0.0 5.0 0.001
GE 0
EX 0 1 26 0 1.0 0.0
FR 0 3 0 0 14.0 1.0
EN
"""


def test_import():
    """Module can be imported."""
    assert hasattr(fnec_py, "solve_deck_str")
    assert hasattr(fnec_py, "sweep_deck_str")


def test_solve_deck_str_returns_dict():
    """solve_deck_str returns a dict with required keys."""
    result = fnec_py.solve_deck_str(DIPOLE_14MHZ)
    assert isinstance(result, dict), f"expected dict, got {type(result)}"
    for key in ("freq_mhz", "tag", "seg", "z_re", "z_im", "z_abs", "z_arg_deg"):
        assert key in result, f"missing key '{key}' in result: {result}"


def test_solve_deck_str_frequency():
    """freq_mhz matches the FR card."""
    result = fnec_py.solve_deck_str(DIPOLE_14MHZ)
    assert abs(result["freq_mhz"] - 14.0) < 1e-6, f"unexpected freq_mhz: {result['freq_mhz']}"


def test_solve_deck_str_impedance_is_real_positive():
    """Near-resonant dipole resistance is positive."""
    result = fnec_py.solve_deck_str(DIPOLE_14MHZ)
    assert result["z_re"] > 0.0, f"z_re should be positive, got {result['z_re']}"
    assert result["z_abs"] > 0.0, f"z_abs should be positive, got {result['z_abs']}"


def test_sweep_deck_str_returns_list():
    """sweep_deck_str returns a list of dicts."""
    results = fnec_py.sweep_deck_str(SWEEP_3FREQ)
    assert isinstance(results, list), f"expected list, got {type(results)}"
    assert len(results) == 3, f"expected 3 records, got {len(results)}"
    for rec in results:
        assert isinstance(rec, dict)
        for key in ("freq_mhz", "tag", "seg", "z_re", "z_im", "z_abs", "z_arg_deg"):
            assert key in rec, f"missing key '{key}' in record: {rec}"


def test_sweep_frequencies_ascending():
    """Frequencies in sweep result are ascending."""
    results = fnec_py.sweep_deck_str(SWEEP_3FREQ)
    freqs = [r["freq_mhz"] for r in results]
    assert freqs == sorted(freqs), f"frequencies not ascending: {freqs}"


def test_invalid_deck_raises_runtime_error():
    """A malformed deck string raises RuntimeError."""
    with pytest.raises(RuntimeError):
        fnec_py.solve_deck_str("NOT A VALID DECK\n")


def test_corpus_dipole_freesp():
    """Solve the corpus free-space dipole and check impedance is in a plausible range."""
    corpus_root = os.path.join(
        os.path.dirname(__file__), "..", "..", "..", "corpus"
    )
    deck_path = os.path.join(corpus_root, "dipole-freesp-51seg.nec")
    with open(deck_path) as f:
        deck = f.read()
    result = fnec_py.solve_deck_str(deck)
    # Free-space half-wave dipole impedance: ~73 + 42j Ω at resonance.
    # Allow generous tolerance for different frequencies.
    assert 10.0 < result["z_re"] < 1000.0, f"implausible z_re: {result['z_re']}"
    assert result["z_abs"] > 0.0

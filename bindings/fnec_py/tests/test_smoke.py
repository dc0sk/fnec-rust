# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Smoke tests for fnec_py Python bindings (PH4-CHK-004)."""

import os
import warnings

import fnec_py
import pytest

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


# --- pre-solve validation parity with the CLI (review-260719 FIND-004) -------

# Two wires crossing at mid-span, neither meeting the other at an endpoint.
CROSSING_WIRES = """\
GW 1 11 -5 0 0 5 0 0 0.001
GW 2 11 0 -5 0 0 5 0 0.001
GE
EX 0 1 6 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
EN
"""

# A vertical wire whose base sits on an active (PEC) ground plane.
BURIED_OVER_PEC = """\
GW 1 21 0 0 0 0 0 10 0.001
GE 1
EX 0 1 11 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
EN
"""

# A half-wave dipole 0.05 lambda over average ground — solvable, but only
# approximately, so it must warn.
LOW_OVER_GROUND = """\
GW 1 21 -5.278 0 1.056 5.278 0 1.056 0.001
GE 1
GN 2 0 0 0 13 0.005
EX 0 1 11 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
EN
"""

# Three wires meeting at the origin: a topology the Hallen solver mis-solves.
TEE_JUNCTION = """\
GW 1 11 -5 0 0 0 0 0 0.001
GW 2 11 0 0 0 5 0 0 0.001
GW 3 11 0 0 0 0 0 5 0.001
GE
EX 0 1 6 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
EN
"""


@pytest.mark.parametrize(
    "deck,fragment",
    [
        (CROSSING_WIRES, "intersecting-wire"),
        (BURIED_OVER_PEC, "buried-wire"),
    ],
)
def test_geometry_the_cli_refuses_is_refused_here_too(deck, fragment):
    """Geometry outside the solver's supported class must raise, not return a number.

    These bindings used to go straight from build_geometry to solve_hallen, so a
    deck the CLI rejects outright produced a plausible-looking impedance here.
    """
    with pytest.raises(RuntimeError, match=fragment):
        fnec_py.solve_deck_str(deck)


def test_a_clean_deck_solves_with_no_warnings():
    """Negative control: the rejection above must not be rejecting everything."""
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        result = fnec_py.solve_deck_str(DIPOLE_14MHZ)
    assert result["z_re"] > 0.0
    assert caught == [], f"unexpected warnings: {[str(w.message) for w in caught]}"


@pytest.mark.parametrize(
    "deck,fragment",
    [
        (LOW_OVER_GROUND, "above finite ground"),
        (TEE_JUNCTION, "--solver mpie"),
    ],
)
def test_solvable_but_caveated_decks_emit_a_user_warning(deck, fragment):
    """A deck that solves unreliably must say so, as a filterable UserWarning."""
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        fnec_py.solve_deck_str(deck)
    messages = [str(w.message) for w in caught]
    assert any(w.category is UserWarning for w in caught), f"not UserWarnings: {caught}"
    assert any(fragment in m for m in messages), f"missing {fragment!r} in {messages}"


def test_sweep_does_not_repeat_the_same_caveat_per_frequency():
    """A geometry caveat is about the geometry, not about one frequency point."""
    deck = LOW_OVER_GROUND.replace(
        "FR 0 1 0 0 14.2 0.0", "FR 0 5 0 0 14.2 0.0"
    )
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        records = fnec_py.sweep_deck_str(deck)
    assert len(records) == 5
    messages = [str(w.message) for w in caught]
    assert len(messages) == len(set(messages)), f"duplicate warnings: {messages}"
    assert len(messages) < len(records), f"one warning per point, not deduped: {messages}"


def test_a_warning_can_be_escalated_to_an_error():
    """The caveats are real Python warnings, so `-W error` style filtering works."""
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        with pytest.raises(UserWarning, match="above finite ground"):
            fnec_py.solve_deck_str(LOW_OVER_GROUND)


# --- cross-frontend parity for NT decks (FND-015) ---------------------------

def test_nt_deck_matches_the_corpus_reference():
    """An `NT` card must change the answer here exactly as it does in the CLI.

    `build_nt_stamps` used to be called only from the CLI, so this deck solved to
    the *plain-dipole* 74.243 + j13.900 here while the CLI gave 70.633 + j14.009 —
    the same deck, two frontends, 3.6 ohm apart. The reference value below is the
    CLI-produced corpus entry, so this assertion is red against the old behaviour.
    """
    import json

    root = os.path.join(os.path.dirname(__file__), "..", "..", "..")
    deck_path = os.path.join(root, "corpus", "dipole-nt-tl-equiv-freesp-51seg.nec")
    with open(deck_path) as f:
        deck = f.read()
    with open(os.path.join(root, "corpus", "reference-results.json")) as f:
        want = json.load(f)["cases"]["dipole-nt-tl-equiv-freesp-51seg"][
            "feedpoint_impedance"
        ]

    got = fnec_py.solve_deck_str(deck)
    assert abs(got["z_re"] - want["real_ohm"]) < 0.05, (
        f"NT stamp missing or wrong: got {got['z_re']}, reference {want['real_ohm']}"
    )
    assert abs(got["z_im"] - want["imag_ohm"]) < 0.05, (
        f"NT stamp missing or wrong: got {got['z_im']}, reference {want['imag_ohm']}"
    )


def test_a_deck_without_nt_is_unaffected():
    """Negative control: the seam must not perturb a deck that has no NT card.

    Read from the corpus rather than hardcoded, so this tracks the same source of
    truth as its sibling above instead of drifting from it.
    """
    import json

    root = os.path.join(os.path.dirname(__file__), "..", "..", "..")
    with open(os.path.join(root, "corpus", "dipole-freesp-51seg.nec")) as f:
        plain = fnec_py.solve_deck_str(f.read())
    with open(os.path.join(root, "corpus", "reference-results.json")) as f:
        want = json.load(f)["cases"]["dipole-freesp-51seg"]["feedpoint_impedance"]
    assert abs(plain["z_re"] - want["real_ohm"]) < 0.05, (
        f"plain dipole moved: got {plain['z_re']}, reference {want['real_ohm']}"
    )


def test_a_negative_resistance_deck_raises_a_warning():
    """A physically impossible result must not be returned silently (FND-014).

    `warn_if_negative_resistance` was private to the CLI with a single call site,
    so this deck solved to a negative feedpoint resistance here with no caveat at
    all — the same number the CLI has flagged since PH9-CHK-005.
    """
    import warnings as pywarnings

    root = os.path.join(os.path.dirname(__file__), "..", "..", "..")
    with open(os.path.join(root, "corpus", "inverted-v-negative-r-freesp.nec")) as f:
        deck = f.read()

    with pywarnings.catch_warnings(record=True) as caught:
        pywarnings.simplefilter("always")
        got = fnec_py.solve_deck_str(deck)

    assert got["z_re"] < 0.0, f"fixture must produce Re Z < 0, got {got['z_re']}"
    texts = [str(w.message) for w in caught]
    assert any("negative resistance" in t for t in texts), (
        f"impossible result returned without a caveat: {texts}"
    )


def test_a_clean_deck_raises_no_negative_resistance_warning():
    """Negative control: the tripwire must not fire on a sound deck."""
    import warnings as pywarnings

    root = os.path.join(os.path.dirname(__file__), "..", "..", "..")
    with open(os.path.join(root, "corpus", "dipole-freesp-51seg.nec")) as f:
        deck = f.read()

    with pywarnings.catch_warnings(record=True) as caught:
        pywarnings.simplefilter("always")
        got = fnec_py.solve_deck_str(deck)

    assert got["z_re"] > 0.0
    texts = [str(w.message) for w in caught]
    assert not any("negative resistance" in t for t in texts), texts


def test_a_plane_wave_is_not_read_as_the_feedpoint():
    """FND-031: the bindings took the first `EX` card of any type.

    A plane wave has no feedpoint — its tag/segment fields carry NTHETA and NPHI —
    so this deck used to report segment 3, a grid dimension, as the antenna
    feedpoint where the CLI reported segment 26.

    Asserts on resolution, not on the impedance: a deck carrying a plane wave is a
    receive deck, and its driven-feedpoint value is degenerate.
    """
    root = os.path.join(os.path.dirname(__file__), "..", "..", "..")
    path = os.path.join(root, "corpus", "dipole-planewave-then-source-51seg.nec")
    with open(path) as f:
        got = fnec_py.solve_deck_str(f.read())

    assert (got["tag"], got["seg"]) == (1, 26), (
        f"resolved the wrong EX: tag={got['tag']} seg={got['seg']}"
    )

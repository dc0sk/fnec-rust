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
        # The remedy names the bindings' own argument now, not the CLI's flag:
        # `fnec_py` gained `solver="mpie"` in FND-055, so pointing a Python caller
        # at a different program stopped being the right advice.
        (TEE_JUNCTION, 'pass solver="mpie"'),
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
    """FND-031, still pinned — but the deck is now refused (FND-050).

    A plane wave has no feedpoint: its tag/segment fields carry NTHETA and NPHI,
    so this deck used to report segment 3, a grid dimension, as the antenna
    feedpoint where the CLI reported segment 26.

    The deck is refused now, because a plane wave beside a driven source is a mix
    fnec will not solve and NEC-2 does not superpose either — nec2c answers it
    identically to the same deck with the plane wave deleted. So the assertion
    moved from the resolved feedpoint to the refusal's text; it is the same fact,
    and it still fails if the plane wave is read as the feedpoint.
    """
    root = os.path.join(os.path.dirname(__file__), "..", "..", "..")
    path = os.path.join(root, "corpus", "dipole-planewave-then-source-51seg.nec")
    with open(path) as f:
        deck = f.read()

    with pytest.raises(Exception) as excinfo:
        fnec_py.solve_deck_str(deck)
    message = str(excinfo.value)
    assert "tag 1 segment 26" in message, (
        f"the refusal must name the voltage source, not the plane wave: {message}"
    )
    assert "plane wave" in message, message


def test_a_current_source_deck_solves_and_agrees_with_the_cli():
    """FND-045: this used to raise "use the fnec CLI for this deck".

    The machinery was in `nec_solver` all along — `solve_hallen_current_source` —
    and the bindings simply never called it. The assertion is the CLI's
    corpus-pinned value, so the two frontends cannot drift apart.
    """
    root = os.path.join(os.path.dirname(__file__), "..", "..", "..")
    with open(os.path.join(root, "corpus", "dipole-ex4-freesp-51seg.nec")) as f:
        got = fnec_py.solve_deck_str(f.read())

    assert abs(got["z_re"] - 74.23) < 0.05, got["z_re"]
    assert abs(got["z_im"] - 13.9) < 0.05, got["z_im"]
    # And it names the current source, not some other EX card.
    assert (got["tag"], got["seg"]) == (1, 26), got


def test_a_deck_with_two_kinds_of_source_is_refused():
    """FND-036: refused rather than answered with a hundredfold error."""
    root = os.path.join(os.path.dirname(__file__), "..", "..", "..")
    with open(os.path.join(root, "corpus", "dipole-mixed-sources-51seg.nec")) as f:
        deck = f.read()

    try:
        fnec_py.solve_deck_str(deck)
    except Exception as e:  # noqa: BLE001 — the message is the assertion
        assert "both a voltage source" in str(e), str(e)
    else:
        raise AssertionError("a mixed-source deck must not solve")



def test_the_bindings_offer_the_mpie_solver():
    """FND-055: `fnec_py` was the last frontend without a solver choice.

    The CLI has `--solver mpie` and the GUI a picker, so a Python caller with a
    degree-3 junction was told to reach for a different program. The assertion is
    the CLI's pinned value, so the frontends cannot drift.
    """
    y_junction = (
        "CM Y-junction, feed mid arm 1\nCE\n"
        "GW 1 20 0.0 0.0 0.0 5.0 0.0 0.0 0.001\n"
        "GW 2 20 0.0 0.0 0.0 -2.5 4.330127 0.0 0.001\n"
        "GW 3 20 0.0 0.0 0.0 -2.5 -4.330127 0.0 0.001\n"
        "GE 0\nFR 0 1 0 0 14.2 0\nEX 0 1 10 0 1.0 0.0\nEN\n"
    )
    got = fnec_py.solve_deck_str(y_junction, solver="mpie")
    assert abs(got["z_re"] - 63.673674) < 0.05, got
    assert abs(got["z_im"] - -322.199211) < 0.05, got


def test_the_default_solver_is_unchanged():
    """Existing callers pass no solver and must get exactly what they got before."""
    deck = (
        "CM dipole\nCE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\n"
        "EX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n"
    )
    default = fnec_py.solve_deck_str(deck)
    explicit = fnec_py.solve_deck_str(deck, solver="hallen")
    assert default == explicit


def test_an_unknown_solver_name_is_refused():
    with pytest.raises(ValueError) as excinfo:
        fnec_py.solve_deck_str("GW 1 21 0 0 -1 0 0 1 0.001\nGE 0\nEN\n", solver="wishful")
    assert "wishful" in str(excinfo.value)


def test_a_sweep_reports_negative_resistance_once_not_per_point():
    """FND-032: the per-point sentence embeds `Re Z = {z_re:.3}`, so every point's
    text differs and dedup on message identity fails. A junctioned sweep raised one
    UserWarning per frequency; the GUI had aggregated and the bindings had not.
    """
    bent = (
        "CM inverted-V fed away from the apex\nCE\n"
        "GW 1 21 -5.0 0 0.0 0.0 0 3.0 0.001\n"
        "GW 2 21 0.0 0 3.0 5.0 0 0.0 0.001\n"
        "GE 0\nEX 0 1 5 0 1.0 0.0\nFR 0 11 0 0 14.0 0.05\nEN\n"
    )
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        fnec_py.sweep_deck_str(bent)
    negative = [w for w in caught if "negative resistance" in str(w.message)]
    assert len(negative) <= 1, (
        f"expected one aggregate line, got {len(negative)}: "
        f"{[str(w.message)[:60] for w in negative]}"
    )
    if negative:
        assert "sweep points report negative" in str(negative[0].message)

#!/usr/bin/env python3
"""Known-pass / known-fail cases for `derive-corpus-provenance.py --check` (FND-177).

The failing case that matters is the one the version check could not see: a
case's stored numbers changed within the release under development, its stamp
untouched. That must be stale; an untouched, freshly stamped case must not.

Run: python3 scripts/test-derive-corpus-provenance.py
"""

from __future__ import annotations

import copy
import importlib.util
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("prov", HERE / "derive-corpus-provenance.py")
prov = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prov)

failures: list[str] = []


def expect(label: str, got, want) -> None:
    if got != want:
        failures.append(f"{label}: got {got!r}, want {want!r}")


def stamped(data: dict, version: str = "0.19.0") -> dict:
    case = dict(data, last_produced_on="2026-09-27", last_produced_in=version)
    case["last_produced_fingerprint"] = prov.case_fingerprint(case)
    return case


dipole = {"deck_file": "d.nec", "feedpoint_impedance": {"real_ohm": 78.825136, "imag_ohm": 42.434844}}
history = {"d": ("2026-09-27", "0.19.0")}

fresh = {"d": stamped(dipole)}
expect("a freshly stamped case", prov.stale_cases(fresh, history), [])

# The FND-177 case: numbers re-pinned in the same version, stamp left as it was.
repinned = copy.deepcopy(fresh)
repinned["d"]["feedpoint_impedance"]["real_ohm"] = 999.0
expect("re-pinned in the same version, not re-stamped", prov.stale_cases(repinned, history), ["d"])

expect(
    "stamped by an older version",
    prov.stale_cases({"d": stamped(dipole, "0.18.0")}, history),
    ["d"],
)
no_date = {"d": dict(stamped(dipole), last_produced_on="")}
expect("no date", prov.stale_cases(no_date, history), ["d"])
no_fp = {"d": {k: v for k, v in stamped(dipole).items() if k != "last_produced_fingerprint"}}
expect("no fingerprint", prov.stale_cases(no_fp, history), ["d"])

# The stamp keys are bookkeeping, not data: rewriting them must not change the hash.
expect(
    "the fingerprint ignores the stamp",
    prov.case_fingerprint(dict(stamped(dipole), last_produced_on="2030-01-01")),
    prov.case_fingerprint(stamped(dipole)),
)

if failures:
    for f in failures:
        print(f"FAIL {f}", file=sys.stderr)
    sys.exit(1)
print("derive-corpus-provenance self-test OK")

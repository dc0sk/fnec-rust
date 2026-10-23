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


# main() end to end (FND-296: the cases above test the helpers only, so a main()
# that never wrote the stamp, or whose --check always passed, went unseen). A
# scratch repository with one case: unstamped --check fails, a stamping run
# writes the stamp and --check passes, a re-pin committed without re-stamping
# fails again.
import json  # noqa: E402
import os  # noqa: E402
import subprocess  # noqa: E402
import tempfile  # noqa: E402


def git(repo: Path, *args: str) -> None:
    subprocess.run(
        ["git", "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false",
         *args],
        cwd=repo, check=True, capture_output=True,
    )


def main_run(repo: Path, *args: str) -> int:
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    return subprocess.run(
        [sys.executable, str(HERE / "derive-corpus-provenance.py"), *args],
        cwd=repo, capture_output=True, text=True, env=env,
    ).returncode


with tempfile.TemporaryDirectory(prefix="prov-") as tmp:
    repo = Path(tmp)
    corpus = repo / "corpus" / "reference-results.json"
    corpus.parent.mkdir()
    (repo / "Cargo.toml").write_text('[workspace.package]\nversion = "0.19.0"\n')
    corpus.write_text(json.dumps({"cases": {"d": copy.deepcopy(dipole)}}))
    git(repo, "init", "-q")
    git(repo, "add", "Cargo.toml", "corpus/reference-results.json")
    git(repo, "commit", "-q", "-m", "one case")
    expect("main --check on an unstamped case", main_run(repo, "--check"), 1)
    expect("main stamps", main_run(repo), 0)
    stamp = json.loads(corpus.read_text())["cases"]["d"]
    expect("main wrote the version", stamp.get("last_produced_in"), "0.19.0")
    expect(
        "main wrote the fingerprint",
        stamp.get("last_produced_fingerprint"),
        prov.case_fingerprint(stamp),
    )
    expect("main --check after stamping", main_run(repo, "--check"), 0)
    doc = json.loads(corpus.read_text())
    doc["cases"]["d"]["feedpoint_impedance"]["real_ohm"] = 999.0
    corpus.write_text(json.dumps(doc))
    git(repo, "commit", "-q", "-am", "re-pin, not re-stamped")
    expect("main --check after a re-pin", main_run(repo, "--check"), 1)

if failures:
    for f in failures:
        print(f"FAIL {f}", file=sys.stderr)
    sys.exit(1)
print("derive-corpus-provenance self-test OK")

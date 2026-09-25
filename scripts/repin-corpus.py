#!/usr/bin/env python3
"""Re-pin the fnec-produced values in `corpus/reference-results.json`.

Most corpus rows pin fnec's own answer, so a deliberate solver change moves them
all at once. This runs every case exactly as `apps/nec-cli/tests/corpus_validation.rs`
does (`fnec --solver hallen <cli_args> <deck>`), reads the output the same way,
and rewrites the stored values that test compares against:

  * `feedpoint_impedance` — `source_N` rows, frequency-keyed rows, or the scalar
    `real_ohm`/`imag_ohm`, in that precedence, as the test reads them;
  * `pattern_samples` — matched on (theta, phi);
  * `current_samples` — matched on (wire_id = tag, segment_id = seg).

It never touches `external_reference_candidate` or `tolerance_gates`: an external
number is not fnec's to rewrite, and a tolerance is a decision, not a measurement.
Cases that expect a Hallén failure are skipped.

Every changed case gets `--note` appended to its `status`, and the old → new
values are printed so the diff can be reviewed before it is committed. Run
`scripts/derive-corpus-provenance.py` after committing to restamp provenance.

Usage:
  scripts/repin-corpus.py --fnec target/debug/fnec --note "Re-pinned ... (FND-NNN): ..."
  scripts/repin-corpus.py --fnec target/debug/fnec --dry-run
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

FILE = Path("corpus/reference-results.json")
CORPUS = Path("corpus")
# A value that moved less than this is fnec's print precision, not a change.
EPS = 5e-7


def run_case(fnec: str, case: dict) -> str:
    cmd = [fnec, "--solver", "hallen", *case.get("cli_args", []), str(CORPUS / case["deck_file"])]
    out = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if out.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)} failed ({out.returncode}):\n{out.stderr}")
    return out.stdout


def impedances(stdout: str) -> list[tuple[float, float]]:
    """Contract-v1 rows: TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM."""
    rows = []
    for line in stdout.splitlines():
        p = line.split()
        if len(p) >= 8 and p[0].isdigit():
            try:
                rows.append((float(p[6]), float(p[7])))
            except ValueError:
                pass
    return rows


def pattern_rows(stdout: str) -> list[list[float]]:
    rows, inside = [], False
    for line in stdout.splitlines():
        if line == "RADIATION_PATTERN":
            inside = True
            continue
        if not inside:
            continue
        if not line:
            break
        p = line.split()
        if len(p) >= 6:
            try:
                rows.append([float(x) for x in p[:6]])
            except ValueError:
                pass
    return rows


def current_rows(stdout: str) -> dict[tuple[int, int], tuple[float, float]]:
    rows, inside = {}, False
    for line in stdout.splitlines():
        if "SEG" in line and "I_MAG" in line:
            inside = True
            continue
        if not inside:
            continue
        if not line or line.startswith("---"):
            break
        p = line.split()
        if len(p) >= 6:
            try:
                rows[(int(p[0]), int(p[1]))] = (float(p[4]), float(p[5]))
            except ValueError:
                pass
    return rows


def stored_resolution(value: float) -> float:
    """Half a unit in the last decimal place the stored value was written with.

    Pins were written at different precisions (74.27, 152.352342). A value that
    still rounds to what is stored has not moved, and rewriting it would bury the
    real changes under print-precision noise.
    """
    text = repr(float(value))
    if "e" in text or "." not in text:
        return EPS
    return 0.5 * 10.0 ** -len(text.split(".")[1]) + EPS


def set_value(obj: dict, key: str, new: float, where: str, changes: list[str]) -> None:
    old = obj.get(key)
    new = round(new, 6)
    if old is not None and abs(old - new) <= stored_resolution(old):
        return
    changes.append(f"{where}.{key}: {old} -> {new}")
    obj[key] = new


def repin(case: dict, stdout: str) -> list[str]:
    changes: list[str] = []
    feed = case.get("feedpoint_impedance", {})
    if not feed:
        # An empty object pins nothing; the corpus test skips these rows too.
        return changes
    z = impedances(stdout)
    if not z:
        raise RuntimeError("no impedance rows in fnec output")
    n_src = case.get("sources") or 0
    src_keys = [f"source_{i}" for i in range(1, n_src + 1)]
    freq_keys = sorted((k for k in feed if _is_float(k)), key=float)
    if n_src and all(k in feed for k in src_keys):
        for i, k in enumerate(src_keys):
            set_value(feed[k], "real_ohm", z[i][0], k, changes)
            set_value(feed[k], "imag_ohm", z[i][1], k, changes)
    elif freq_keys:
        for i, k in enumerate(freq_keys):
            set_value(feed[k], "real_ohm", z[i][0], k, changes)
            set_value(feed[k], "imag_ohm", z[i][1], k, changes)
    if "real_ohm" in feed and "imag_ohm" in feed:
        set_value(feed, "real_ohm", z[0][0], "Z", changes)
        set_value(feed, "imag_ohm", z[0][1], "Z", changes)

    if case.get("pattern_samples"):
        rows = pattern_rows(stdout)
        for s in case["pattern_samples"]:
            row = next(
                (r for r in rows if abs(r[0] - s["theta_deg"]) < 1e-9 and abs(r[1] - s["phi_deg"]) < 1e-9),
                None,
            )
            if row is None:
                raise RuntimeError(f"no pattern row at theta={s['theta_deg']} phi={s['phi_deg']}")
            where = f"rp({s['theta_deg']},{s['phi_deg']})"
            for key, val in zip(("gain_db", "gain_v_db", "gain_h_db", "axial_ratio"), row[2:6]):
                set_value(s, key, val, where, changes)

    if case.get("current_samples"):
        rows = current_rows(stdout)
        for s in case["current_samples"]:
            row = rows.get((s["wire_id"], s["segment_id"]))
            if row is None:
                raise RuntimeError(f"no current row for tag {s['wire_id']} seg {s['segment_id']}")
            where = f"I({s['wire_id']},{s['segment_id']})"
            set_value(s, "amplitude_db", row[0], where, changes)
            set_value(s, "phase_deg", row[1], where, changes)
    return changes


def _is_float(s: str) -> bool:
    try:
        float(s)
        return True
    except ValueError:
        return False


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--fnec", required=True, help="fnec binary to produce the values")
    ap.add_argument("--note", help="sentence appended to each changed case's status")
    ap.add_argument("--dry-run", action="store_true", help="report, do not write")
    args = ap.parse_args()
    if not args.dry_run and not args.note:
        ap.error("--note is required unless --dry-run: a re-pin must say why")

    doc = json.loads(FILE.read_text(encoding="utf-8"))
    changed = 0
    for name in sorted(doc["cases"]):
        case = doc["cases"][name]
        if case.get("expected_hallen_error_contains"):
            continue
        if "feedpoint_impedance" not in case:
            continue
        try:
            changes = repin(case, run_case(args.fnec, case))
        except RuntimeError as e:
            raise SystemExit(f"{name}: {e}") from None
        if changes:
            changed += 1
            print(f"{name}:")
            for c in changes:
                print(f"  {c}")
            if args.note:
                case["status"] = f"{case.get('status', '').rstrip()} {args.note}".strip()
    print(f"{changed} case(s) changed")
    if not args.dry_run:
        FILE.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())

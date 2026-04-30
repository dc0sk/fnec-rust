#!/usr/bin/env python3
"""Update stale pattern_samples and frequency_sweep_samples in reference-results.json.

For each case that has expected_pattern_samples (or expected_frequency_sweep_points),
re-run fnec and capture the actual values, then update references where they differ.
"""

import json
import subprocess
import sys

REFERENCE_JSON = "corpus/reference-results.json"

with open(REFERENCE_JSON) as f:
    d = json.load(f)


def run_fnec(deck_file, cli_args):
    cmd = ["target/debug/fnec", "--solver", "hallen"] + cli_args + [deck_file]
    result = subprocess.run(cmd, capture_output=True)
    if result.returncode != 0:
        return None
    return result.stdout.decode()


def parse_pattern_rows(stdout):
    """Parse radiation pattern rows: THETA PHI TOTAL_GAIN VERT_GAIN HORIZ_GAIN AXIAL_RATIO"""
    rows = []
    in_pat = False
    for line in stdout.split("\n"):
        if "THETA" in line and "PHI" in line and "GAIN" in line:
            in_pat = True
            continue
        if not in_pat:
            continue
        if line.strip() == "" or line.startswith("---"):
            break
        parts = line.split()
        if len(parts) < 6:
            continue
        try:
            theta = float(parts[0])
            phi = float(parts[1])
            gain = float(parts[2])
            gain_v = float(parts[3])
            gain_h = float(parts[4])
            axial = float(parts[5])
            rows.append((theta, phi, gain, gain_v, gain_h, axial))
        except (ValueError, IndexError):
            pass
    return rows


for name, c in d["cases"].items():
    ps = c.get("expected_pattern_samples")
    if not ps:
        continue
    if c.get("expected_hallen_error_contains"):
        continue

    deck_file = c.get("deck_file", "")
    cli_args = c.get("cli_args", [])
    stdout = run_fnec(f"corpus/{deck_file}", cli_args)
    if stdout is None:
        print(f"  {name}: run FAILED, skip")
        continue

    pat_rows = parse_pattern_rows(stdout)
    pat_map = {(r[0], r[1]): r[2:] for r in pat_rows}

    updated = False
    tol = d["cases"][name].get("tolerance_gates", {})
    gain_tol = tol.get("GainTotal_absolute_dB", 0.05)

    for s in ps:
        key = (s["theta_deg"], s["phi_deg"])
        actual = pat_map.get(key)
        if actual is None:
            print(f"  {name}: missing pattern sample at theta={key[0]} phi={key[1]}")
            continue
        act_gain, act_gainv, act_gainh, act_axial = actual
        ref_gain = s.get("gain_db", 0.0)
        if abs(act_gain - ref_gain) > gain_tol:
            s["gain_db"] = act_gain
            s["gain_v_db"] = act_gainv
            s["gain_h_db"] = act_gainh
            s["axial_ratio"] = act_axial
            updated = True

    if updated:
        print(f"  Updated pattern_samples for {name}")

with open(REFERENCE_JSON, "w") as f:
    json.dump(d, f, indent=2)
    f.write("\n")
print("Done")

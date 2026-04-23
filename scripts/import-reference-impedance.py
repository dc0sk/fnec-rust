#!/usr/bin/env python3
"""Import reference impedance values into corpus/reference-results.json.

This script updates one corpus case at a time and optionally supports nested
feedpoint keys (for sweep/multi-source cases).

Examples:
  scripts/import-reference-impedance.py \
    --case dipole-ground-51seg \
    --real 63.12 --imag -18.45 \
    --source "4nec2 (Wine 9.x)" \
    --status "Reference captured via 4nec2/Wine"

  scripts/import-reference-impedance.py \
    --case frequency-sweep-dipole \
    --point 12 \
    --real 41.21 --imag -28.34 \
    --source "4nec2 (Windows VM)"
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
from pathlib import Path
import sys


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Update corpus/reference-results.json with impedance values"
    )
    parser.add_argument(
        "--file",
        default="corpus/reference-results.json",
        help="Path to reference JSON file (default: corpus/reference-results.json)",
    )
    parser.add_argument("--case", required=True, help="Case key under .cases")
    parser.add_argument(
        "--point",
        help=(
            "Optional sub-key under feedpoint_impedance, e.g. '12' for sweep "
            "or 'source_1' for multi-source"
        ),
    )
    parser.add_argument("--real", required=True, type=float, help="Real impedance in ohms")
    parser.add_argument("--imag", required=True, type=float, help="Imag impedance in ohms")
    parser.add_argument(
        "--source",
        help="Reference source string to set as case.reference_source",
    )
    parser.add_argument(
        "--status",
        help="Optional case status string to update",
    )
    parser.add_argument(
        "--engine",
        help="Optional top-level reference_engine override",
    )
    parser.add_argument(
        "--engine-version",
        help="Optional top-level reference_engine_version override",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    json_path = Path(args.file)
    if not json_path.exists():
        print(f"error: file not found: {json_path}", file=sys.stderr)
        return 1

    data = json.loads(json_path.read_text(encoding="utf-8"))
    cases = data.get("cases")
    if not isinstance(cases, dict):
        print("error: invalid schema: missing object 'cases'", file=sys.stderr)
        return 1

    if args.case not in cases:
        available = ", ".join(sorted(cases.keys()))
        print(
            f"error: unknown case '{args.case}'. available: {available}",
            file=sys.stderr,
        )
        return 1

    case_obj = cases[args.case]
    feed = case_obj.get("feedpoint_impedance")
    if not isinstance(feed, dict):
        print(
            f"error: invalid schema: case '{args.case}' missing object 'feedpoint_impedance'",
            file=sys.stderr,
        )
        return 1

    if args.point:
        if args.point not in feed or not isinstance(feed[args.point], dict):
            print(
                f"error: case '{args.case}' has no point '{args.point}' under feedpoint_impedance",
                file=sys.stderr,
            )
            return 1
        feed[args.point]["real_ohm"] = args.real
        feed[args.point]["imag_ohm"] = args.imag
    else:
        feed["real_ohm"] = args.real
        feed["imag_ohm"] = args.imag

    if args.source:
        case_obj["reference_source"] = args.source
    if args.status:
        case_obj["status"] = args.status
    if args.engine:
        data["reference_engine"] = args.engine
    if args.engine_version:
        data["reference_engine_version"] = args.engine_version

    data["generated_date"] = dt.date.today().isoformat()

    json_path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")

    where = f"{args.case}.{args.point}" if args.point else args.case
    print(
        f"updated {where}: Z = {args.real:.6f} {args.imag:+.6f}j ohm in {json_path}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

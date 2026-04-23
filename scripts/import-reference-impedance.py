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

    scripts/import-reference-impedance.py \
        --batch-file .tmp-work/reference-import.json
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
    parser.add_argument("--case", help="Case key under .cases")
    parser.add_argument(
        "--point",
        help=(
            "Optional sub-key under feedpoint_impedance, e.g. '12' for sweep "
            "or 'source_1' for multi-source"
        ),
    )
    parser.add_argument("--real", type=float, help="Real impedance in ohms")
    parser.add_argument("--imag", type=float, help="Imag impedance in ohms")
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
    parser.add_argument(
        "--batch-file",
        help=(
            "Path to JSON file with bulk updates. If provided, single-case flags "
            "(--case/--real/--imag/--point) are ignored."
        ),
    )
    return parser.parse_args()


def update_single_case(cases: dict, case_name: str, point: str | None, real: float, imag: float) -> None:
    if case_name not in cases:
        available = ", ".join(sorted(cases.keys()))
        raise ValueError(f"unknown case '{case_name}'. available: {available}")

    case_obj = cases[case_name]
    feed = case_obj.get("feedpoint_impedance")
    if not isinstance(feed, dict):
        raise ValueError(
            f"invalid schema: case '{case_name}' missing object 'feedpoint_impedance'"
        )

    if point:
        if point not in feed or not isinstance(feed[point], dict):
            raise ValueError(
                f"case '{case_name}' has no point '{point}' under feedpoint_impedance"
            )
        feed[point]["real_ohm"] = real
        feed[point]["imag_ohm"] = imag
    else:
        feed["real_ohm"] = real
        feed["imag_ohm"] = imag


def apply_batch_updates(data: dict, batch_path: Path) -> int:
    if not batch_path.exists():
        raise ValueError(f"batch file not found: {batch_path}")

    payload = json.loads(batch_path.read_text(encoding="utf-8"))
    updates = payload.get("updates")
    if not isinstance(updates, list):
        raise ValueError("batch file must contain an 'updates' array")

    cases = data.get("cases")
    if not isinstance(cases, dict):
        raise ValueError("invalid schema: missing object 'cases'")

    count = 0
    for idx, item in enumerate(updates, start=1):
        if not isinstance(item, dict):
            raise ValueError(f"updates[{idx}] must be an object")

        case_name = item.get("case")
        point = item.get("point")
        real = item.get("real")
        imag = item.get("imag")
        if not isinstance(case_name, str):
            raise ValueError(f"updates[{idx}] missing string 'case'")
        if not isinstance(real, (int, float)):
            raise ValueError(f"updates[{idx}] missing numeric 'real'")
        if not isinstance(imag, (int, float)):
            raise ValueError(f"updates[{idx}] missing numeric 'imag'")

        update_single_case(cases, case_name, point, float(real), float(imag))

        case_obj = cases[case_name]
        if isinstance(item.get("source"), str):
            case_obj["reference_source"] = item["source"]
        if isinstance(item.get("status"), str):
            case_obj["status"] = item["status"]
        count += 1

    if isinstance(payload.get("engine"), str):
        data["reference_engine"] = payload["engine"]
    if isinstance(payload.get("engine_version"), str):
        data["reference_engine_version"] = payload["engine_version"]

    return count


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

    try:
        if args.batch_file:
            updated_count = apply_batch_updates(data, Path(args.batch_file))
            where = f"batch:{updated_count}"
        else:
            if args.case is None or args.real is None or args.imag is None:
                print(
                    "error: --case, --real and --imag are required unless --batch-file is used",
                    file=sys.stderr,
                )
                return 1

            update_single_case(cases, args.case, args.point, args.real, args.imag)
            case_obj = cases[args.case]
            if args.source:
                case_obj["reference_source"] = args.source
            if args.status:
                case_obj["status"] = args.status
            where = f"{args.case}.{args.point}" if args.point else args.case

        if args.engine:
            data["reference_engine"] = args.engine
        if args.engine_version:
            data["reference_engine_version"] = args.engine_version
    except ValueError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1

    data["generated_date"] = dt.date.today().isoformat()

    json_path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")

    if args.batch_file:
        print(f"updated {where} entries in {json_path}")
    else:
        print(f"updated {where}: Z = {args.real:.6f} {args.imag:+.6f}j ohm in {json_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

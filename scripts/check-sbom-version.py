#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""The SBOM describes the release being tagged: every workspace member is in
`SBOM.spdx.json` at the workspace version.

FND-068: the version-bump gate requires the SBOM to CHANGE with a bump, but
that gate runs only on pull requests, and nothing at release time looked inside
the file. A stale SBOM — regenerated for the previous version, or not at all —
could be tagged. The release workflow runs this against the tree it tags.

Usage (from the tree to check):  scripts/check-sbom-version.py
"""

import json
import sys
import tomllib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from cargo_version import declared_version  # noqa: E402  (one parser, FND-067)


def main() -> int:
    manifest = Path("Cargo.toml").read_text(encoding="utf-8")
    version = declared_version(manifest)
    if version is None:
        print("check-sbom-version: no workspace version in Cargo.toml", file=sys.stderr)
        return 1
    members = tomllib.loads(manifest)["workspace"]["members"]
    names = [
        tomllib.loads(Path(m, "Cargo.toml").read_text(encoding="utf-8"))["package"]["name"]
        for m in members
    ]
    sbom = json.loads(Path("SBOM.spdx.json").read_text(encoding="utf-8"))
    have = {}
    for p in sbom.get("packages", []):
        have.setdefault(p.get("name"), set()).add(p.get("versionInfo"))
    problems = []
    for n in names:
        if n not in have:
            problems.append(f"{n} is not in SBOM.spdx.json")
        elif version not in have[n]:
            problems.append(f"{n} is in SBOM.spdx.json at {sorted(have[n])}, not {version}")
    if problems:
        print(f"SBOM does not describe {version}:", *problems, sep="\n  ", file=sys.stderr)
        return 1
    print(f"SBOM OK — {len(names)} workspace member(s) at {version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

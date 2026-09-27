#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""The workspace version declared by a `Cargo.toml` — parsed, not grepped.

One implementation for every script that needs it (FND-067, FND-068). Two of
them took the first line starting with `version` and split on quotes, the
extraction `check-release-tags.py` already documented as a trap: it works on
every manifest today, which is exactly how a check earns false confidence, and a
check that stamps and verifies with the same wrong extraction certifies itself.

Usage as a program:  scripts/cargo_version.py <git-ref>
  prints the version of `<git-ref>:Cargo.toml`, exit 1 if there is none.
"""

import subprocess
import sys
import tomllib


def declared_version(toml_text: str) -> str | None:
    """The package version, wherever this era of the manifest keeps it.

    At HEAD the workspace version lives under `[workspace.package]`; older
    manifests keep it under `[package]`.
    """
    try:
        data = tomllib.loads(toml_text)
    except tomllib.TOMLDecodeError:
        return None
    for table in (("workspace", "package"), ("package",)):
        node = data
        for key in table:
            node = node.get(key, {}) if isinstance(node, dict) else {}
        if isinstance(node, dict) and isinstance(node.get("version"), str):
            return node["version"]
    return None


def version_at(ref: str) -> str | None:
    """The workspace version at a git ref, or None if it has no manifest."""
    out = subprocess.run(
        ["git", "show", f"{ref}:Cargo.toml"], capture_output=True, text=True, check=False
    )
    if out.returncode != 0:
        return None
    return declared_version(out.stdout)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    v = version_at(sys.argv[1])
    if v is None:
        sys.exit(1)
    print(v)

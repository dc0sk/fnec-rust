#!/usr/bin/env python3
"""The wheel's version must match the crate's.

`bindings/fnec_py` declares its version twice: in `Cargo.toml`, and in
`pyproject.toml` — and it is the *pyproject* one that maturin stamps onto the
built wheel, which is what a user installs and what `pip show` reports.

They drifted, and it shipped. v0.14.0's release notes announced
"fnec_py 0.4.0 -> 0.5.0" while `pyproject.toml` still said 0.4.0, so the wheel
carried 0.5.0's breaking behaviour under 0.4.0's name — the exact opposite of
what a version number is for. The release-checklist's own consistency command
greps `Cargo.toml` files, so it could not see it (FND-044).
"""

import re
import sys
import tomllib
from pathlib import Path

CARGO = Path("bindings/fnec_py/Cargo.toml")
PYPROJECT = Path("bindings/fnec_py/pyproject.toml")


def main() -> int:
    for f in (CARGO, PYPROJECT):
        if not f.exists():
            print(f"{f} is missing", file=sys.stderr)
            return 1

    cargo = tomllib.loads(CARGO.read_text(encoding="utf-8"))["package"]["version"]
    pyproject = tomllib.loads(PYPROJECT.read_text(encoding="utf-8"))["project"]["version"]

    if cargo != pyproject:
        print(
            f"fnec_py version mismatch: {CARGO} says {cargo}, "
            f"{PYPROJECT} says {pyproject}.\n"
            f"The wheel takes its version from {PYPROJECT}, so a user would "
            f"install {pyproject} while the crate believes it is {cargo}.",
            file=sys.stderr,
        )
        return 1

    if not re.fullmatch(r"\d+\.\d+\.\d+", cargo):
        print(f"unexpected version format: {cargo}", file=sys.stderr)
        return 1

    print(f"fnec_py version OK — {CARGO.name} and {PYPROJECT.name} both say {cargo}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

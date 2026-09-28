#!/usr/bin/env python3
"""Refuse release assets whose minimum runtime platform rose without a decision.

A native artifact inherits the build host's libc. v0.19.0's new `sinh`/`cosh`
calls bound GLIBC_2.44 on a rolling-release host: the host-built wheel would have
stopped loading on every older distro (0.18.0's needed 2.35), and maturin only
warned. The binaries beside it had already drifted 2.43 -> 2.44 between releases
without anyone noticing, because nothing compared them (FND-168).

The baseline is `docs/project/release-asset-platform.toml`. Every ELF asset, and
every `.so` inside a wheel, must require no GLIBC symbol version newer than
`glibc_max`; every wheel must carry a manylinux tag no newer than it. Raising the
floor is allowed — by editing the baseline in a commit, and saying so in the
release notes — never by an asset quietly arriving above it.

A missing `objdump` fails the check: an absent tool must not read as a clean
artifact.

Usage:
  scripts/check-asset-platform.py <asset-dir>
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
import zipfile
from pathlib import Path

BASELINE = Path(__file__).resolve().parent.parent / "docs/project/release-asset-platform.toml"
GLIBC_RE = re.compile(r"GLIBC_(\d+)\.(\d+)(?:\.(\d+))?")
WHEEL_TAG_RE = re.compile(r"-(?P<plat>(?:manylinux|musllinux|linux)[\w.]*)\.whl$")
MANYLINUX_RE = re.compile(r"manylinux_(\d+)_(\d+)_")


def version(text: str) -> tuple[int, ...]:
    return tuple(int(p) for p in text.split("."))


def max_glibc(objdump_text: str) -> tuple[int, ...] | None:
    """Highest GLIBC_x.y[.z] in `objdump -T` output; None when it names none."""
    found = [tuple(int(g) for g in m.groups() if g is not None) for m in GLIBC_RE.finditer(objdump_text)]
    return max(found) if found else None


def wheel_manylinux(name: str) -> tuple[int, ...] | None:
    """The oldest glibc a wheel's platform tag promises; None when it promises none."""
    m = WHEEL_TAG_RE.search(name)
    if not m:
        return None
    tags = [(int(a), int(b)) for a, b in MANYLINUX_RE.findall(m.group("plat") + "_")]
    # `manylinux2014` alone is the legacy alias of manylinux_2_17.
    if not tags and "manylinux2014" in m.group("plat"):
        tags = [(2, 17)]
    return min(tags) if tags else None


def objdump(path: Path, tool: str | None) -> str:
    if tool is None:
        raise RuntimeError("objdump not found: cannot read the symbol versions, refusing to pass")
    out = subprocess.run([tool, "-T", str(path)], capture_output=True, text=True)
    if out.returncode != 0:
        raise RuntimeError(f"objdump -T {path} failed: {out.stderr.strip()}")
    return out.stdout


def is_elf(path: Path) -> bool:
    with path.open("rb") as f:
        return f.read(4) == b"\x7fELF"


def check(asset_dir: Path, ceiling: tuple[int, ...], tool: str | None) -> list[str]:
    """Return one failure message per offending asset; print a row per asset."""
    failures: list[str] = []
    fmt = ".".join(map(str, ceiling))
    assets = sorted(p for p in asset_dir.iterdir() if p.is_file())
    if not assets:
        return [f"{asset_dir}: no assets"]
    for p in assets:
        if p.suffix == ".whl":
            tag = wheel_manylinux(p.name)
            if tag is None:
                failures.append(f"{p.name}: no manylinux tag — a host build (build it in the container)")
            elif tag > ceiling:
                failures.append(f"{p.name}: tag promises glibc {'.'.join(map(str, tag))} > baseline {fmt}")
            with zipfile.ZipFile(p) as z, tempfile.TemporaryDirectory() as tmp:
                sos = [n for n in z.namelist() if n.endswith(".so")]
                if not sos:
                    failures.append(f"{p.name}: no extension module inside")
                for n in sos:
                    so = Path(z.extract(n, tmp))
                    got = max_glibc(objdump(so, tool))
                    shown = ".".join(map(str, got)) if got else "none"
                    print(f"  {p.name} :: {n}: GLIBC {shown}")
                    if got and got > ceiling:
                        failures.append(f"{p.name} :: {n}: requires GLIBC_{shown} > baseline {fmt}")
        elif is_elf(p):
            got = max_glibc(objdump(p, tool))
            shown = ".".join(map(str, got)) if got else "none"
            print(f"  {p.name}: GLIBC {shown}")
            if got and got > ceiling:
                failures.append(f"{p.name}: requires GLIBC_{shown} > baseline {fmt}")
    return failures


def main(argv: list[str]) -> int:
    if len(argv) != 2 or not Path(argv[1]).is_dir():
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    ceiling = version(tomllib.loads(BASELINE.read_text(encoding="utf-8"))["glibc_max"])
    try:
        failures = check(Path(argv[1]), ceiling, shutil.which("objdump"))
    except RuntimeError as e:
        print(f"asset platform: {e}", file=sys.stderr)
        return 1
    if failures:
        for f in failures:
            print(f"asset platform: {f}", file=sys.stderr)
        print(
            f"asset platform FAILED — baseline glibc_max {'.'.join(map(str, ceiling))} in {BASELINE.name}. "
            "Build in the container (scripts/build-release-assets.sh), or raise the baseline in a "
            "commit and say so in the release notes.",
            file=sys.stderr,
        )
        return 1
    print(f"asset platform OK — every asset within glibc {'.'.join(map(str, ceiling))}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

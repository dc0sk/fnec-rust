#!/usr/bin/env python3
"""Known-pass / known-fail cases for `check-asset-platform.py` (FND-168).

The failing cases are the real ones: the host-built 0.19.0 wheel tagged plain
`linux_x86_64`, an extension that binds GLIBC_2.44, and a host without objdump.
Each must fail; the container-built 0.19.0 names must pass.

Run: python3 scripts/test-check-asset-platform.py
"""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("cap", HERE / "check-asset-platform.py")
cap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cap)

failures: list[str] = []


def expect(label: str, got, want) -> None:
    if got != want:
        failures.append(f"{label}: got {got!r}, want {want!r}")


# --- symbol-version parsing (objdump -T text) ---
OBJDUMP_OLD = """
0000000000000000      DF *UND*  0000000000000000 (GLIBC_2.2.5) malloc
0000000000000000      DF *UND*  0000000000000000 (GLIBC_2.14) memcpy
"""
OBJDUMP_244 = OBJDUMP_OLD + "0000000000000000      DF *UND*  0000000000000000 (GLIBC_2.44) sinh\n"
expect("max of 2.2.5 and 2.14", cap.max_glibc(OBJDUMP_OLD), (2, 14))
expect("2.44 beats 2.14 numerically, not lexically", cap.max_glibc(OBJDUMP_244), (2, 44))
expect("no GLIBC symbols", cap.max_glibc("no versions here"), None)

# --- wheel platform tags ---
expect(
    "0.19.0 container wheel",
    cap.wheel_manylinux("fnec_py-0.10.0-cp314-cp314-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"),
    (2, 17),
)
expect("0.18.0 wheel", cap.wheel_manylinux("fnec_py-0.9.0-cp314-cp314-manylinux_2_35_x86_64.whl"), (2, 35))
expect("host-built wheel has no promise", cap.wheel_manylinux("fnec_py-0.10.0-cp314-cp314-linux_x86_64.whl"), None)
expect("legacy alias alone", cap.wheel_manylinux("x-1.0-cp314-cp314-manylinux2014_x86_64.whl"), (2, 17))

# --- check(): whole directories, with a fake objdump ---
CEILING = (2, 17)


def run_check(files: dict[str, bytes], wheel_so: str | None, tool_text: str | None) -> list[str]:
    """`tool_text` is what the fake objdump prints for every file; None = objdump missing."""
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp) / "assets"
        d.mkdir()
        for name, data in files.items():
            (d / name).write_bytes(data)
        if wheel_so is not None:
            with zipfile.ZipFile(d / wheel_so, "w") as z:
                z.writestr("fnec_py/fnec_py.cpython-314-x86_64-linux-gnu.so", b"\x7fELF")
        real = cap.objdump
        cap.objdump = lambda path, tool: real(path, None) if tool_text is None else tool_text
        try:
            return cap.check(d, CEILING, None if tool_text is None else "fake")
        except RuntimeError as e:
            return [f"raised: {e}"]
        finally:
            cap.objdump = real


ELF = b"\x7fELF" + b"\0" * 12
GOOD_WHEEL = "fnec_py-0.10.0-cp314-cp314-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"
expect("container-built set passes", run_check({"fnec-v0.19.0-x86_64-linux": ELF}, GOOD_WHEEL, OBJDUMP_OLD), [])
expect(
    "a binary requiring 2.44 fails",
    len(run_check({"fnec-v0.19.0-x86_64-linux": ELF}, None, OBJDUMP_244)),
    1,
)
host_wheel = run_check({}, "fnec_py-0.10.0-cp314-cp314-linux_x86_64.whl", OBJDUMP_244)
expect("host-built wheel: no tag AND its .so above the floor", len(host_wheel), 2)
expect(
    "a wheel promising 2.35 fails against a 2.17 floor",
    len(run_check({}, "fnec_py-0.9.0-cp314-cp314-manylinux_2_35_x86_64.whl", OBJDUMP_OLD)),
    1,
)
missing = run_check({"fnec-v0.19.0-x86_64-linux": ELF}, None, None)
expect("missing objdump fails closed", len(missing) == 1 and missing[0].startswith("raised:"), True)
expect("an empty directory fails", len(run_check({}, None, OBJDUMP_OLD)), 1)
expect("a non-ELF text file is ignored", run_check({"SBOM-v0.19.0.spdx.json": b"{}"}, GOOD_WHEEL, OBJDUMP_OLD), [])

if failures:
    for f in failures:
        print(f"FAIL {f}", file=sys.stderr)
    sys.exit(1)
print("check-asset-platform self-test OK")

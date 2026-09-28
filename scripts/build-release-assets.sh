#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# Build the four release assets — CLI, GUI, Python wheel, SBOM — in one pinned
# manylinux container, then refuse them if their minimum glibc rose (FND-168).
#
# Why a container. A native artifact inherits the build host's libc. Built on this
# rolling-release host, 0.19.0's new `sinh`/`cosh` calls bound GLIBC_2.44: the
# wheel would have stopped loading on every older distro (0.18.0's needed 2.35),
# and the binaries had already drifted 2.43 -> 2.44 between releases unnoticed.
# The container is manylinux2014 (glibc 2.17): 0.19.0 built there needs 2.16 for
# the binaries and 2.14 for the wheel, and gives the same answers to the digit.
#
# Usage: scripts/build-release-assets.sh [--dry-run] [--out DIR]
#   HEAD must sit exactly on tag v<workspace version> with a clean tree, unless
#   --dry-run (which still builds and checks, for trying the pipeline before a tag).
#   Assets land in DIR (default ~/.cache/fnec-release-v<version>). The cargo
#   target and registry go to ~/.cache/fnec-release-build — outside the worktree
#   and off /tmp, which is a tmpfs here. Delete both once the release is published.
set -euo pipefail

# The image, by digest: `latest` moves, and a moved image is a changed toolchain
# floor nobody chose. It carries rustup; the toolchain itself comes from
# rust-toolchain.toml, so the container builds with the same pinned rustc as CI.
IMAGE="ghcr.io/pyo3/maturin@sha256:0d1711efd2d3aab276f738bdcda48d378bceef2eeb7f8a8caf59451defddee72"
PYTHON="python3.14"

dry_run=0
out=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) dry_run=1; shift ;;
        --out) out="$2"; shift 2 ;;
        *) echo "usage: $0 [--dry-run] [--out DIR]" >&2; exit 2 ;;
    esac
done

repo="$(git rev-parse --show-toplevel)"
cd "$repo"
version="$(python3 scripts/cargo_version.py HEAD)"
out="${out:-$HOME/.cache/fnec-release-v$version}"
build="$HOME/.cache/fnec-release-build"

if [[ $dry_run -eq 0 ]]; then
    tag="$(git describe --exact-match --tags HEAD 2>/dev/null || true)"
    [[ "$tag" == "v$version" ]] || { echo "HEAD is not on tag v$version (got '${tag:-none}'); use --dry-run to try the build" >&2; exit 1; }
    [[ -z "$(git status --porcelain)" ]] || { echo "the tree has uncommitted changes; a release builds from the tag alone" >&2; exit 1; }
fi

# Never inside the worktree (`git add` would sweep it) and never on a tmpfs (a
# workspace build fills it, and then every shell command fails bare).
for d in "$out" "$build"; do
    real="$(realpath -m "$d")"
    case "$real" in
        "$repo"|"$repo"/*) echo "$d is inside the worktree; build output belongs outside it" >&2; exit 1 ;;
    esac
    if [[ "$(df --output=fstype "$(dirname "$real")" 2>/dev/null | tail -1)" == "tmpfs" ]]; then
        echo "$d is on a tmpfs; build output belongs on disk" >&2; exit 1
    fi
done

engine="$(command -v podman || command -v docker || true)"
[[ -n "$engine" ]] || { echo "neither podman nor docker is installed" >&2; exit 1; }

mkdir -p "$out" "$build/target" "$build/cargo-home" "$build/wheel"
rm -f "$build"/wheel/*.whl

# shellcheck source=scripts/host-build-lock.sh
source scripts/host-build-lock.sh "build-release-assets $version"

run() {
    "$engine" run --rm \
        -v "$repo":/io:ro,Z \
        -v "$build/target":/target:Z \
        -v "$build/cargo-home":/cargo-home:Z \
        -v "$build/wheel":/wheel:Z \
        -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/target \
        -e PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 \
        --entrypoint /bin/bash "$IMAGE" -c "$1"
}

echo "== building v$version in $IMAGE"
run 'cd /io && rustc -V && cargo build --locked --release -p nec-cli -p nec-gui'
run "cd /io/bindings/fnec_py && maturin build --locked --release -i $PYTHON --out /wheel"

rm -f "$out"/*
cp "$build/target/release/fnec" "$out/fnec-v$version-x86_64-linux"
cp "$build/target/release/fnec-gui" "$out/fnec-gui-v$version-x86_64-linux"
cp "$build"/wheel/*.whl "$out/"
cp SBOM.spdx.json "$out/SBOM-v$version.spdx.json"

# The CLI must say the version it is shipped as. Exit status first: before
# FND-169 `--version` printed usage headed `fnec <version>` and exited 2, and a
# first-line check read that as a version report.
if ! reported="$("$out/fnec-v$version-x86_64-linux" --version)"; then
    echo "the CLI's --version failed" >&2; exit 1
fi
[[ "$reported" == "fnec $version" ]] || { echo "the CLI reports '$reported', not 'fnec $version'" >&2; exit 1; }

# And it must run and reproduce the corpus pin for the reference dipole.
python3 - "$out/fnec-v$version-x86_64-linux" <<'PY'
import json, subprocess, sys
case = json.load(open("corpus/reference-results.json"))["cases"]["dipole-freesp-51seg"]
pin, gate = case["feedpoint_impedance"], case["tolerance_gates"]
run = subprocess.run([sys.argv[1], "corpus/" + case["deck_file"]], capture_output=True, text=True)
if run.returncode != 0:
    sys.exit(f"the built CLI failed on the reference dipole (exit {run.returncode}): {run.stderr.strip()[:300]}")
lines = run.stdout.splitlines()
row = lines[lines.index("FEEDPOINTS") + 2].split()
r, x = float(row[6]), float(row[7])
if abs(r - pin["real_ohm"]) > gate["R_absolute_ohm"] or abs(x - pin["imag_ohm"]) > gate["X_absolute_ohm"]:
    sys.exit(f"the built CLI gives {r}+j{x} on the reference dipole; the corpus pins {pin}")
print(f"reference dipole: {r}+j{x} (pin {pin['real_ohm']}+j{pin['imag_ohm']})")
PY

python3 scripts/check-sbom-version.py
python3 scripts/check-asset-platform.py "$out"
echo "== assets in $out:"
ls -l "$out"

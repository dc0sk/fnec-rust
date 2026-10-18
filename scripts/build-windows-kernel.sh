#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# Build fnec for 64-bit Windows, to install as 4nec2's kernel (PAR-011 P3).
#
#   scripts/build-windows-kernel.sh [--out DIR]
#
# Cross-compiles `nec-cli` for x86_64-pc-windows-gnu in a rust container pinned by
# digest (the toolchain matches rust-toolchain.toml), with Debian's mingw-w64.
# Output goes outside the worktree (default ~/.cache/fnec-windows-kernel): `fnec.exe`,
# the six copies 4nec2 runs (it picks its kernel by segment count, so all six are
# replaced, see docs/4nec2-drop-in.md), and BUILD-INFO with the image digest, the
# linker version and every file's sha256.
#
# The import table is checked against the DLLs every Windows 10+ install ships:
# wgpu links dxgi.dll and opengl32.dll at load time, both system DLLs; anything
# else would make the kernel fail to start on a machine without it, so the build
# refuses it.
#
# Tested only under Wine 10.0 (win64 prefix) with 4nec2 V5.9.3, never on Windows.
# The binary is unsigned. Not a release asset.
set -euo pipefail

IMAGE="docker.io/library/rust:1.98.1-bookworm@sha256:c49256cbe5ea0188bc658a689500d70c41eb51f009a7a7be209caf60a944f3ec"
TARGET="x86_64-pc-windows-gnu"
NAMES=(nec2dxs500 nec2dxs1K5 nec2dxs3k0 nec2dxs5k0 nec2dxs8k0 nec2dxs11k)
# Every DLL the build may import: present on every Windows 10 and later.
ALLOWED="api-ms-win-core-synch-l1-2-0.dll api-ms-win-core-winrt-error-l1-1-0.dll \
bcryptprimitives.dll combase.dll dxgi.dll gdi32.dll kernel32.dll msvcrt.dll ntdll.dll \
oleaut32.dll opengl32.dll rpcrt4.dll setupapi.dll user32.dll userenv.dll ws2_32.dll"

out="$HOME/.cache/fnec-windows-kernel"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --out) out="$2"; shift 2 ;;
        *) echo "usage: $0 [--out DIR]" >&2; exit 2 ;;
    esac
done

repo="$(git rev-parse --show-toplevel)"
real="$(realpath -m "$out")"
case "$real" in
    "$repo"|"$repo"/*) echo "$out is inside the worktree; build output belongs outside it" >&2; exit 1 ;;
esac
command -v podman >/dev/null || { echo "podman is needed" >&2; exit 1; }
command -v objdump >/dev/null || { echo "objdump (binutils) is needed for the import check" >&2; exit 1; }
mkdir -p "$out/target"

# The container cannot read .git: name the commit (and a dirty tree) here, as
# build.rs would (FND-264).
commit="$("$repo/scripts/build-commit.sh")"
podman run --rm -v "$repo":/src:ro -v "$out":/out -e CARGO_TARGET_DIR=/out/target \
    -e FNEC_BUILD_COMMIT="$commit" "$IMAGE" bash -c "
    set -euo pipefail
    apt-get update -qq && apt-get install -y -qq gcc-mingw-w64-x86-64 >/dev/null
    rustup target add $TARGET >/dev/null
    dpkg-query -W -f='mingw-w64 \${Version}\n' gcc-mingw-w64-x86-64 > /out/linker-version
    rustc --version > /out/rustc-version
    cd /src && cargo build --release --locked --target $TARGET -p nec-cli
"
exe="$out/target/$TARGET/release/fnec.exe"
[[ -f "$exe" ]] || { echo "no $exe" >&2; exit 1; }

bad=""
while read -r dll; do
    lower="${dll,,}"
    [[ " $ALLOWED " == *" $lower "* ]] || bad="$bad $dll"
done < <(objdump -p "$exe" | sed -n 's/^\s*DLL Name: //p' | sort -u)
if [[ -n "$bad" ]]; then
    echo "fnec.exe imports DLLs Windows does not ship everywhere:$bad" >&2
    exit 1
fi

cp "$exe" "$out/fnec.exe"
for n in "${NAMES[@]}"; do cp "$exe" "$out/$n.exe"; done
{
    echo "fnec Windows kernel build"
    echo "commit $(git -C "$repo" rev-parse HEAD)"
    echo "build $commit"
    echo "image $IMAGE"
    cat "$out/rustc-version" "$out/linker-version"
    (cd "$out" && sha256sum fnec.exe "${NAMES[@]/%/.exe}")
} > "$out/BUILD-INFO"
cat "$out/BUILD-INFO"

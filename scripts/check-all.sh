#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# The full local gate, across BOTH cargo trees (FND-024).
#
# `bindings/fnec_py` is deliberately excluded from the workspace — it is a cdylib
# with its own lockfile — so every `--workspace` command run at the root skips it
# entirely. That makes the obvious local gate a liar: `cargo fmt --all --check` at
# the root exits 0 on an unformatted bindings crate, and CI then fails on it.
# Measured, both directions:
#
#     misformat in crates/nec_solver   root `fmt --all --check` = 1   bindings = 1
#     misformat in bindings/fnec_py    root `fmt --all --check` = 0   bindings = 1
#
# The asymmetry is the whole trick: the bindings crate pulls the workspace crates
# in as *path* dependencies, so ONE `cargo fmt --all --check` run from
# `bindings/fnec_py` covers both trees, while the root run covers only its own.
# That is why the fmt step below runs there and not at the root.
#
# Usage:  scripts/check-all.sh [--fast]
#   --fast   skip the test suite (fmt, clippy and the doc checkers only)

set -uo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
FAST=0
[[ "${1:-}" == "--fast" ]] && FAST=1

FAILED=()
run() {
    local name="$1"; shift
    printf '%-34s' "$name"
    if "$@" >/tmp/check-all-step.log 2>&1; then
        echo "ok"
    else
        echo "FAILED (exit $?)"
        sed 's/^/    /' /tmp/check-all-step.log | tail -25
        FAILED+=("$name")
    fi
}

# One invocation, both trees — see the note above.
run "fmt (both trees)" bash -c "cd '$ROOT/bindings/fnec_py' && cargo fmt --all --check"

run "clippy (workspace)" cargo clippy --workspace --all-targets -- -D warnings

# pyo3 0.23 supports CPython up to 3.13; a newer local interpreter makes the build
# fail outright. CI pins 3.13 and must NOT set this — it is a local escape hatch,
# not something to gate on.
run "clippy (fnec_py)" bash -c \
    "cd '$ROOT/bindings/fnec_py' && PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo clippy --all-targets -- -D warnings"

if [[ $FAST -eq 0 ]]; then
    run "test (workspace)" cargo test --workspace
fi

for c in check-changelog-headings check-findings-ledger check-path-inventory \
         check-release-tags check-binding-version; do
    run "$c" python3 "scripts/$c.py"
done

echo
if [[ ${#FAILED[@]} -eq 0 ]]; then
    echo "all gates passed"
    exit 0
fi
echo "FAILED: ${FAILED[*]}"
exit 1

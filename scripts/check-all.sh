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

    # The bindings crate is outside the workspace, so `cargo test --workspace`
    # never reached it and NOTHING ran its Rust tests — `clippy --all-targets`
    # above compiles them and walks away. CI runs pytest against a built wheel,
    # which needs maturin and an interpreter pyo3 supports; these run anywhere.
    # Same asymmetry as the fmt note at the top of this file (FND-024).
    run "test (fnec_py)" bash -c \
        "cd '$ROOT/bindings/fnec_py' && PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo test"
fi

for c in check-changelog-headings check-findings-ledger check-path-inventory \
         check-release-tags check-binding-version; do
    run "$c" python3 "scripts/$c.py"
done

# The three CI enforces that this script did not, so a stale artifact could only
# ever be caught after a push. All three are --check modes of generators, so the
# fix when one fails is to run the generator without --check and commit.
#
# Found the hard way: the corpus provenance stamps for three LD cases still said
# "produced in 0.3.0 on 2026-04-30" after FND-122 re-derived them on 2026-08-29,
# and the first thing to notice was a red `docs contract` job on the PR. Those
# stamps ARE the evidence-expiry mechanism — a validation dated before the change
# it validates is exactly what they exist to surface — so a stale one is a real
# defect, not paperwork.
run "docs frontmatter" bash scripts/validate-docs-frontmatter.sh
run "traceability matrix fresh" python3 scripts/gen-traceability-matrix.py --check
run "corpus provenance fresh" python3 scripts/derive-corpus-provenance.py --check

# The one checker with a committed self-test, and it only ran in CI — so a defect
# in the checker itself reached a release and was found by reading its output by
# hand (FND-062). A gate whose own test runs somewhere else is a gate you trust
# for reasons you cannot see locally.
run "check-release-tags self-test" python3 scripts/test-check-release-tags.py

# Against the merge base, so it sees the doc-regression half — a doc comment can
# only come adrift from an item *relative to* where that item was documented.
BASE="$(git merge-base HEAD origin/main 2>/dev/null || echo '')"
if [[ -n "$BASE" ]]; then
    run "check-doc-attachment" python3 scripts/check-doc-attachment.py --base "$BASE"
else
    run "check-doc-attachment" python3 scripts/check-doc-attachment.py
fi

echo
if [[ ${#FAILED[@]} -eq 0 ]]; then
    echo "all gates passed"
    exit 0
fi
echo "FAILED: ${FAILED[*]}"
exit 1

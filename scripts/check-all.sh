#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# The full local gate, across BOTH cargo trees (FND-024): every check the CI
# workflow runs, run the same way (FND-066). The one CI job left out by default
# is coverage, an instrumented rebuild of the whole suite; `--coverage` adds it.
# An earlier version called itself "the full local gate" while omitting eight CI
# checks, the bindings pytest among them — which was then believed to be
# CI-only and cost two CI round trips on stale pinned values.
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
# Usage:  scripts/check-all.sh [--fast] [--coverage]
#   --fast       skip the test suites (fmt, clippy, audit/deny and the doc checkers)
#   --coverage   also run CI's coverage gate (cargo llvm-cov, >= 75% lines)

set -uo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
FAST=0
COVERAGE=0
for arg in "$@"; do
    case "$arg" in
        --fast) FAST=1 ;;
        --coverage) COVERAGE=1 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

# Queue behind any other heavy build on this machine, in any project, rather
# than race it for RAM. See the helper for why this is host-wide.
# shellcheck source=scripts/host-build-lock.sh
source "$ROOT/scripts/host-build-lock.sh" "check-all"

pytest_bindings() {
    local venv="$ROOT/target/pytest-venv" module="$ROOT/target/pytest-module"
    if ! "$venv/bin/python" -c 'import pytest' 2>/dev/null; then
        python3 -m venv "$venv" && "$venv/bin/pip" install -q pytest || return 1
    fi
    (cd "$ROOT/bindings/fnec_py" && PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo build -q) || return 1
    mkdir -p "$module" &&
        cp "$ROOT/bindings/fnec_py/target/debug/libfnec_py.so" "$module/fnec_py.abi3.so" || return 1
    PYTHONPATH="$module" "$venv/bin/python" -m pytest -q "$ROOT/bindings/fnec_py/tests"
}

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

    # CI's pytest, without maturin: a scratch venv under target/ (off the tmpfs,
    # ignored by git) and the cargo-built library copied onto PYTHONPATH under
    # the name Python imports. The first run needs network for `pip install`.
    run "pytest (fnec_py)" pytest_bindings

    # Checks the counts in docs/project/test-catalog.md against what the
    # harness lists; it builds nothing the suite above has not already built.
    run "test-catalog counts" python3 scripts/check-test-catalog-counts.py
fi

run "cargo audit" cargo audit
run "cargo deny" cargo deny check bans licenses sources

for c in check-changelog-headings check-findings-ledger check-path-inventory \
         check-release-tags check-binding-version; do
    run "$c" python3 "scripts/$c.py"
done
# Local too: the hooks and this script lint with the `rustc` on PATH, so a
# host a version ahead of the pin gates with a clippy CI does not run (FND-149).
run "toolchain pin (CI + local)" python3 scripts/check-toolchain-pin.py --local
run "SBOM describes the version" python3 scripts/check-sbom-version.py

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
# On a scratch lock file, so it runs inside this gate while the gate holds the
# real one (the re-entrancy it checks is what lets it).
run "host-build-lock self-test" bash scripts/test-host-build-lock.sh
run "corpus provenance self-test" python3 scripts/test-derive-corpus-provenance.py
run "check-asset-platform self-test" python3 scripts/test-check-asset-platform.py

# Against the merge base, so it sees the doc-regression half — a doc comment can
# only come adrift from an item *relative to* where that item was documented.
BASE="$(git merge-base HEAD origin/main 2>/dev/null || echo '')"
if [[ -n "$BASE" ]]; then
    run "check-doc-attachment" python3 scripts/check-doc-attachment.py --base "$BASE"
    # CI runs this on pull requests, against the PR base; the merge base is
    # the local equivalent.
    run "version-bump docs" bash scripts/check-version-bump-docs.sh "$BASE" HEAD
else
    run "check-doc-attachment" python3 scripts/check-doc-attachment.py
fi

if [[ $COVERAGE -eq 1 ]]; then
    run "coverage (>= 75% lines)" cargo llvm-cov --workspace --summary-only --fail-under-lines 75
fi

echo
if [[ ${#FAILED[@]} -eq 0 ]]; then
    echo "all gates passed"
    exit 0
fi
echo "FAILED: ${FAILED[*]}"
exit 1

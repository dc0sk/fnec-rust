#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# Known-pass / known-fail cases for `host-build-lock.sh`, on a scratch lock file
# (never the real one, so this can run inside a gate that holds it).
#
#   1. Nested: a script that takes the lock, then runs a script that takes it
#      too, must not wait on itself. It did, for an hour, on 2026-09-29.
#   2. Contention: an unrelated process must still wait while another holds it —
#      re-entrancy must not become "the lock never blocks".
#   3. Fail closed: an unopenable lock path refuses to run unlocked.
#   4. Worktree build dir: in a linked worktree, a target dir inside it refuses
#      (cargo's default, or CARGO_TARGET_DIR pointing in); one outside passes,
#      and so does the main checkout's own target/.
set -uo pipefail

HELPER="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/host-build-lock.sh"
SCRATCH="$(mktemp -d "${HOME}/.cache/test-host-build-lock.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT
export HEAVY_BUILD_LOCK="$SCRATCH/lock"
fail=0

# 1. Nested, three levels deep. `timeout` turns a deadlock into a failure.
cat > "$SCRATCH/inner.sh" <<EOF
source "$HELPER" inner
echo INNER_RAN
EOF
cat > "$SCRATCH/outer.sh" <<EOF
source "$HELPER" outer
bash "$SCRATCH/inner.sh"
EOF
out="$(timeout 10 bash -c "source '$HELPER' shell; bash '$SCRATCH/outer.sh'" 2>&1)"
rc=$?
if [[ $rc -ne 0 || "$out" != *INNER_RAN* ]]; then
    echo "FAIL nested take: exit $rc" >&2; echo "$out" >&2; fail=1
elif [[ "$out" == *waiting* ]]; then
    echo "FAIL nested take waited on itself:" >&2; echo "$out" >&2; fail=1
fi

# 2. An unrelated holder: the second process must wait (and time out here).
( source "$HELPER" holder; sleep 4 ) 2>/dev/null &
holder=$!
sleep 1
out="$(timeout 2 bash -c "source '$HELPER' stranger; echo STRANGER_RAN" 2>&1)"
rc=$?
if [[ $rc -ne 124 || "$out" == *STRANGER_RAN* || "$out" != *waiting* ]]; then
    echo "FAIL contention: an unrelated process did not wait (exit $rc):" >&2; echo "$out" >&2; fail=1
fi
wait "$holder" 2>/dev/null

# 3. Fail closed on a lock path that cannot be opened.
out="$(HEAVY_BUILD_LOCK="$SCRATCH/no/such/dir/lock" bash -c "source '$HELPER' closed; echo RAN_UNLOCKED" 2>&1)"
rc=$?
if [[ $rc -eq 0 || "$out" == *RAN_UNLOCKED* ]]; then
    echo "FAIL fail-closed: ran without the lock (exit $rc):" >&2; echo "$out" >&2; fail=1
fi

# 4. A scratch crate with a linked worktree, each carrying a copy of the helper
#    (it finds its repo from its own path). The caller's own CARGO_TARGET_DIR is
#    dropped so every case names its target explicitly.
mkdir -p "$SCRATCH/main/scripts" "$SCRATCH/main/src"
cp "$HELPER" "$SCRATCH/main/scripts/"
printf '[package]\nname = "hbl-probe"\nversion = "0.1.0"\nedition = "2021"\n' > "$SCRATCH/main/Cargo.toml"
: > "$SCRATCH/main/src/lib.rs"
git -C "$SCRATCH/main" init -q
git -C "$SCRATCH/main" add Cargo.toml src scripts
git -C "$SCRATCH/main" -c user.name=t -c user.email=t@t -c core.hooksPath=/dev/null commit -q -m probe
git -C "$SCRATCH/main" worktree add -q "$SCRATCH/wt"
probe() { # name want(run|refuse) checkout [CARGO_TARGET_DIR]
    local env=(env -u CARGO_TARGET_DIR -u CARGO_BUILD_TARGET_DIR)
    [[ -n "${4:-}" ]] && env+=("CARGO_TARGET_DIR=$4")
    out="$("${env[@]}" bash -c "cd '$3' && source scripts/host-build-lock.sh wt; echo WT_RAN" 2>&1)"
    rc=$?
    if [[ $2 == run && ( $rc -ne 0 || "$out" != *WT_RAN* ) ]] \
        || [[ $2 == refuse && ( $rc -eq 0 || "$out" == *WT_RAN* || "$out" != *"linked worktree"* ) ]]; then
        echo "FAIL worktree $1: want $2, exit $rc:" >&2; echo "$out" >&2; fail=1
    fi
}
probe "default target" refuse "$SCRATCH/wt"
probe "target pointed inside" refuse "$SCRATCH/wt" "$SCRATCH/wt/build"
probe "target outside" run "$SCRATCH/wt" "$SCRATCH/out"
probe "main checkout" run "$SCRATCH/main"

if [[ $fail -ne 0 ]]; then exit 1; fi
echo "host-build-lock self-test OK"

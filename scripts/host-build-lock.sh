# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# Take the HOST-WIDE heavy-build lock, held until the calling script exits.
# Sourced, not executed: `source scripts/host-build-lock.sh "<who>"`.
#
# Why host-wide and not per repo. A full `cargo test --workspace` here peaks at
# several GB while it links, and nothing coordinates two of them on one machine.
# On 2026-09-25 this repo's gate was OOM-killed three times while another
# project's `cargo test --workspace` ran alongside it, and each kill looked like
# a failure of the code under test. A lock per repo would not have helped, since
# the two runs were in different repositories. The lock file is shared by every
# project whose gate sources an equivalent of this, so they queue instead.
#
# Two rules that keep it from deadlocking:
#   - a script takes it ONCE, near the top; nothing it calls takes it again;
#   - do not wrap a script that takes it in `flock` yourself — the inner take
#     would wait forever on the lock the outer one holds.
#
# Override the path with HEAVY_BUILD_LOCK (all projects must agree on it). If
# `flock` is not installed, the gate runs unlocked and says so.

_hbl_who="${1:-gate}"
_hbl_path="${HEAVY_BUILD_LOCK:-${XDG_RUNTIME_DIR:-/tmp}/heavy-build.lock}"

if command -v flock >/dev/null 2>&1; then
    exec 9>"$_hbl_path"
    if ! flock -n 9; then
        echo "$_hbl_who: another heavy build holds $_hbl_path — waiting for it to finish" >&2
        flock 9
        echo "$_hbl_who: lock acquired, continuing" >&2
    fi
else
    echo "$_hbl_who: flock not installed — running WITHOUT the host-wide build lock" >&2
fi

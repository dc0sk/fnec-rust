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
# Re-entrant within one process tree. The lock lives on descriptor 9, which a
# child inherits, and a flock belongs to the open file description: when fd 9
# already refers to this lock file and re-locking it succeeds at once, an
# ancestor holds the lock for us, and taking it again would only wait on
# ourselves. It used to: a shell that sourced this and then ran `check-all.sh`
# (which sources it too) opened a SECOND description, waited on its own parent
# for an hour, and queued another project's pre-push behind it (2026-09-29).
# An unrelated process still opens its own description and still waits.
#
# One rule remains: do not wrap a script that takes it in `flock` YOURSELF on a
# different descriptor — that take is invisible here, and the inner one would
# wait forever on it.
#
# Override the path with HEAVY_BUILD_LOCK (all projects must agree on it). If
# `flock` is not installed, the gate runs unlocked and says so.

_hbl_who="${1:-gate}"
_hbl_path="${HEAVY_BUILD_LOCK:-${XDG_RUNTIME_DIR:-/tmp}/heavy-build.lock}"

# Refuse to build inside a linked worktree. Cargo's default target/ there is a
# second full build (11–12 GB here) in a directory `git add` can sweep, and an
# exported CARGO_TARGET_DIR in one shell does not cover the cargo a `git commit`
# hook starts: twice on 2026-10-18 the pre-commit clippy built target/ inside a
# worktree. Asked of cargo itself, since .cargo/config.toml can move the target
# too. The main checkout's target/ is the ordinary one and is not checked.
#
# And refuse a target dir another checkout builds into (FND-279). Cargo hashes a
# path package workspace-relative, so two worktrees of this repo are ONE compile
# unit in a shared dir, and the tree whose files are older than the last build
# is judged Fresh: its `cargo test` runs the other tree's code. On 2026-10-20 a
# pre-push gate read the pre-fix FND-270 value that way, and passed three other
# pushes that may have tested nothing of theirs. The first checkout to build in
# a dir claims it in `.fnec-checkout`; any other checkout is refused.
_hbl_repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if _hbl_top="$(git -C "$_hbl_repo" rev-parse --show-toplevel 2>/dev/null)"; then
    _hbl_top="$(realpath "$_hbl_top")"
    if ! _hbl_target="$(cd "$_hbl_top" && cargo metadata --no-deps --format-version 1 2>/dev/null |
            python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"; then
        echo "$_hbl_who: cannot ask cargo where $_hbl_top builds — refusing to build blind" >&2
        exit 1
    fi
    _hbl_target="$(realpath -m "$_hbl_target")"
    if [[ "$(git -C "$_hbl_repo" rev-parse --path-format=absolute --git-dir 2>/dev/null)" \
          != "$(git -C "$_hbl_repo" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" ]]; then
        case "$_hbl_target" in
            "$_hbl_top" | "$_hbl_top"/*)
                echo "$_hbl_who: $_hbl_top is a linked worktree and cargo would build in $_hbl_target, inside it." >&2
                echo "$_hbl_who: export CARGO_TARGET_DIR=\$HOME/.cache/fnec-target-$(basename "$_hbl_top") (outside the worktree, off /tmp) and retry." >&2
                exit 1 ;;
        esac
    fi
    _hbl_owner_file="$_hbl_target/.fnec-checkout"
    if [[ -f "$_hbl_owner_file" ]]; then
        _hbl_owner="$(cat "$_hbl_owner_file")"
        if [[ "$_hbl_owner" != "$_hbl_top" ]]; then
            echo "$_hbl_who: $_hbl_target is the build dir of $_hbl_owner, another checkout; cargo would judge this one's older files Fresh and run that tree's code." >&2
            echo "$_hbl_who: give $_hbl_top its own: export CARGO_TARGET_DIR=\$HOME/.cache/fnec-target-$(basename "$_hbl_top") (or delete $_hbl_target if $_hbl_owner is gone)." >&2
            exit 1
        fi
    elif ! { mkdir -p "$_hbl_target" && printf '%s\n' "$_hbl_top" > "$_hbl_owner_file"; }; then
        echo "$_hbl_who: cannot claim $_hbl_target for $_hbl_top — refusing to build in a dir another checkout could share" >&2
        exit 1
    fi
fi

# Fail CLOSED once flock exists: a lock file that cannot be opened (its directory
# missing, or the file owned by another user) made both `flock` calls fail with
# EBADF, and the old code printed "waiting" and then "lock acquired" and ran
# unlocked — the one outcome this helper exists to prevent, reported as success.
# Does this process tree already hold it? fd 9 inherited from an ancestor that
# took this lock. `readlink` runs as a child, so /proc/self is a process that
# inherited fd 9 too.
_hbl_inherited=""
if [[ -e /proc/self/fd/9 ]] \
    && [[ "$(readlink -f /proc/self/fd/9 2>/dev/null)" == "$(readlink -f "$_hbl_path" 2>/dev/null)" ]] \
    && command -v flock >/dev/null 2>&1 && flock -n 9; then
    _hbl_inherited=1
fi

if [[ -n "$_hbl_inherited" ]]; then
    echo "$_hbl_who: this process tree already holds $_hbl_path — continuing" >&2
elif command -v flock >/dev/null 2>&1; then
    if ! exec 9>"$_hbl_path"; then
        echo "$_hbl_who: cannot open the host-wide build lock $_hbl_path — refusing to run unlocked" >&2
        exit 1
    fi
    if ! flock -n 9; then
        echo "$_hbl_who: another heavy build holds $_hbl_path — waiting for it to finish" >&2
        if ! flock 9; then
            echo "$_hbl_who: could not take $_hbl_path — refusing to run unlocked" >&2
            exit 1
        fi
        echo "$_hbl_who: lock acquired, continuing" >&2
    fi
else
    echo "$_hbl_who: flock not installed — running WITHOUT the host-wide build lock" >&2
fi

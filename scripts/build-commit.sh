#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# Print the FNEC_BUILD_COMMIT value apps/nec-cli/build.rs would compute: twelve hex
# digits of HEAD, "-dirty" when fnec's inputs differ from it (untracked files
# included), "unknown" outside a git checkout. The container builds
# (build-release-assets.sh, build-windows-kernel.sh) cannot read .git, so they run
# this on the host and pass the value in. The path list is build.rs's INPUTS —
# keep the two in step.
set -euo pipefail
cd "$(dirname "$0")/.."
head="$(git rev-parse HEAD 2>/dev/null)" || { echo unknown; exit 0; }
status="$(GIT_OPTIONAL_LOCKS=0 git status --porcelain --untracked-files=normal -- \
    :/crates :/apps/nec-cli :/Cargo.toml :/Cargo.lock :/rust-toolchain.toml)"
if [[ -n "$status" ]]; then echo "${head:0:12}-dirty"; else echo "${head:0:12}"; fi

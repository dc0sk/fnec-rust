#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/pi-remote-workspace-check.sh <ssh-target> [remote-repo-subdir]

Sync the current workspace to a remote Linux host and run cargo test --workspace there.
Requires SSH access (agent-based auth is supported) and rsync on both ends.

Arguments:
  ssh-target      Required remote SSH target, for example: user@192.168.1.10
  remote-repo-subdir Optional remote path under HOME (default: git/fnec-rust)

Environment overrides:
  FNEC_LOCAL_DIR      Local workspace to sync (default: current directory)
  FNEC_TEST_COMMAND   Remote test command (default: cargo test --workspace)
  FNEC_BOOTSTRAP_RUST Install rustup toolchain if cargo is missing (default: 1)
EOF
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage >&2
  exit 1
fi

ssh_target="$1"
remote_repo_subdir="${2:-git/fnec-rust}"
local_dir="${FNEC_LOCAL_DIR:-$PWD}"
test_command="${FNEC_TEST_COMMAND:-cargo test --workspace}"
bootstrap_rust="${FNEC_BOOTSTRAP_RUST:-1}"

if [[ ! -f "${local_dir}/Cargo.toml" ]]; then
  echo "error: ${local_dir} does not look like a Cargo workspace root (missing Cargo.toml)" >&2
  exit 1
fi

echo "[1/3] Syncing workspace to ${ssh_target}:~/${remote_repo_subdir}"
rsync -az --delete \
  --exclude target \
  --exclude .git \
  "${local_dir}/" "${ssh_target}:~/${remote_repo_subdir}/"

echo "[2/3] Ensuring Rust toolchain on ${ssh_target}"
if [[ "${bootstrap_rust}" == "1" ]]; then
  ssh -o BatchMode=yes "${ssh_target}" '
    set -e
    if [[ ! -x "$HOME/.cargo/bin/cargo" ]]; then
      curl https://sh.rustup.rs -sSf | sh -s -- -y
    fi
    . "$HOME/.cargo/env"
    rustc -V
    cargo -V
  '
else
  ssh -o BatchMode=yes "${ssh_target}" '
    set -e
    . "$HOME/.cargo/env"
    rustc -V
    cargo -V
  '
fi

echo "[3/3] Running remote tests: ${test_command}"
ssh -o BatchMode=yes "${ssh_target}" "set -e; . \"\$HOME/.cargo/env\"; cd \"\$HOME/${remote_repo_subdir}\"; ${test_command}"

echo "Remote workspace validation passed on ${ssh_target}."

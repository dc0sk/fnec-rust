#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/pi-remote-benchmark.sh <ssh-target> [remote-repo-subdir]

Sync the current workspace to a remote Linux host and run repeatable nec-cli
benchmarks on selected corpus decks/solvers. Results are written as CSV.

Arguments:
  ssh-target          Optional remote SSH target, for example: user@192.168.1.10
                      If omitted, FNEC_BENCH_TARGET is used.
  remote-repo-subdir  Optional remote path under HOME
                      (default: FNEC_REMOTE_REPO_SUBDIR or git/fnec-rust)

Environment overrides:
  FNEC_LOCAL_DIR       Local workspace to sync (default: current directory)
  FNEC_BENCH_TARGET    Default SSH target when ssh-target argument is omitted
  FNEC_REMOTE_REPO_SUBDIR
                       Default remote path under HOME (default: git/fnec-rust)
  FNEC_BOOTSTRAP_RUST  Install rustup if cargo is missing (default: 1)
  FNEC_BENCH_DECKS     Space-separated deck paths relative to workspace
                       default: "corpus/dipole-freesp-51seg.nec corpus/dipole-ground-51seg.nec corpus/yagi-5elm-51seg.nec"
  FNEC_BENCH_SOLVERS   Space-separated solver names
                       default: "hallen pulse sinusoidal"
  FNEC_BENCH_EXECS     Space-separated execution modes (cpu|hybrid|gpu)
                       default: "cpu"
  FNEC_BENCH_RUNS      Number of repeated runs per deck+solver (default: 3)
  FNEC_BENCH_OUT       Output CSV path (default: tmp/pi-benchmark-<UTC timestamp>.csv)
EOF
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -gt 2 ]]; then
  usage >&2
  exit 1
fi

ssh_target="${1:-${FNEC_BENCH_TARGET:-}}"
remote_repo_subdir="${2:-${FNEC_REMOTE_REPO_SUBDIR:-git/fnec-rust}}"
local_dir="${FNEC_LOCAL_DIR:-$PWD}"
bootstrap_rust="${FNEC_BOOTSTRAP_RUST:-1}"
bench_decks="${FNEC_BENCH_DECKS:-corpus/dipole-freesp-51seg.nec corpus/dipole-ground-51seg.nec corpus/yagi-5elm-51seg.nec}"
bench_solvers="${FNEC_BENCH_SOLVERS:-hallen pulse sinusoidal}"
bench_execs="${FNEC_BENCH_EXECS:-cpu}"
bench_runs="${FNEC_BENCH_RUNS:-3}"
out_csv="${FNEC_BENCH_OUT:-tmp/pi-benchmark-$(date -u +%Y%m%dT%H%M%SZ).csv}"
out_dir="$(dirname "${out_csv}")"

if [[ -z "${ssh_target}" ]]; then
  echo "error: missing ssh-target argument and FNEC_BENCH_TARGET is not set" >&2
  usage >&2
  exit 1
fi

if [[ ! -f "${local_dir}/Cargo.toml" ]]; then
  echo "error: ${local_dir} does not look like a Cargo workspace root (missing Cargo.toml)" >&2
  exit 1
fi

if ! [[ "${bench_runs}" =~ ^[0-9]+$ ]] || [[ "${bench_runs}" -lt 1 ]]; then
  echo "error: FNEC_BENCH_RUNS must be a positive integer" >&2
  exit 1
fi

mkdir -p "${out_dir}"
raw_tsv="$(mktemp)"
trap 'rm -f "${raw_tsv}"' EXIT

echo "[1/4] Syncing workspace to ${ssh_target}:~/${remote_repo_subdir}"
rsync -az --delete \
  --exclude target \
  --exclude .git \
  "${local_dir}/" "${ssh_target}:~/${remote_repo_subdir}/"

echo "[2/4] Ensuring Rust toolchain on ${ssh_target}"
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

echo "[3/4] Running remote benchmarks"
ssh -o BatchMode=yes "${ssh_target}" \
  "REMOTE_REPO_SUBDIR='${remote_repo_subdir}' BENCH_DECKS='${bench_decks}' BENCH_SOLVERS='${bench_solvers}' BENCH_EXECS='${bench_execs}' BENCH_RUNS='${bench_runs}' bash -s" <<'EOF' > "${raw_tsv}"
set -euo pipefail

extract_field() {
  local key="$1"
  local diag_line="$2"
  awk -v k="${key}=" '{ for (i = 1; i <= NF; i++) { if (index($i, k) == 1) { sub(k, "", $i); print $i; exit } } }' <<<"${diag_line}"
}

. "$HOME/.cargo/env"
cd "$HOME/${REMOTE_REPO_SUBDIR}"
cargo build -q -p nec-cli

for deck in ${BENCH_DECKS}; do
  for solver in ${BENCH_SOLVERS}; do
    for exec_mode in ${BENCH_EXECS}; do
      run=1
      while [[ "${run}" -le "${BENCH_RUNS}" ]]; do
        ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        start_ns="$(date +%s%N)"
        status="ok"

        if ! target/debug/fnec --solver "${solver}" --exec "${exec_mode}" "${deck}" >/tmp/fnec_out.txt 2>/tmp/fnec_err.txt; then
          status="fail"
        fi

        end_ns="$(date +%s%N)"
        elapsed_ms="$(( (end_ns - start_ns) / 1000000 ))"

        diag_line="$(awk '/^diag: / { print; exit }' /tmp/fnec_err.txt || true)"
        mode="$(extract_field mode "${diag_line}")"
        pulse_rhs="$(extract_field pulse_rhs "${diag_line}")"
        freq_mhz="$(extract_field freq_mhz "${diag_line}")"
        abs_res="$(extract_field abs_res "${diag_line}")"
        rel_res="$(extract_field rel_res "${diag_line}")"
        diag_spread="$(extract_field diag_spread "${diag_line}")"
        sin_rel_res="$(extract_field sin_rel_res "${diag_line}")"

        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
          "${ts}" "${deck}" "${solver}" "${run}" "${status}" "${elapsed_ms}" \
          "${mode}" "${pulse_rhs}" "${freq_mhz}" "${abs_res}" "${rel_res}" "${exec_mode}" "${diag_spread}" "${sin_rel_res}"

        run="$((run + 1))"
      done
    done
  done
done
EOF

echo "[4/4] Writing CSV to ${out_csv}"
{
  echo "timestamp_utc,target,deck,solver,run,status,elapsed_ms,diag_mode,pulse_rhs,freq_mhz,abs_res,rel_res,exec_mode,diag_spread,sin_rel_res"
  while IFS=$'\t' read -r ts deck solver run status elapsed mode pulse_rhs freq_mhz abs_res rel_res exec_mode diag_spread sin_rel_res; do
    printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
      "${ts}" "${ssh_target}" "${deck}" "${solver}" "${run}" "${status}" "${elapsed}" \
      "${mode}" "${pulse_rhs}" "${freq_mhz}" "${abs_res}" "${rel_res}" "${exec_mode}" "${diag_spread}" "${sin_rel_res}"
  done < "${raw_tsv}"
} > "${out_csv}"

echo "Benchmark summary (ok rows only):"
awk -F, '
  NR == 1 { next }
  $6 != "ok" { next }
  {
    key = $3 "|" $4 "|" ((NF >= 13 && $13 != "") ? $13 : "cpu")
    count[key] += 1
    total[key] += $7
    if (!(key in min) || $7 < min[key]) {
      min[key] = $7
    }
    if (!(key in max) || $7 > max[key]) {
      max[key] = $7
    }
  }
  END {
    for (k in count) {
      split(k, parts, "|")
      avg = total[k] / count[k]
      printf "  deck=%s solver=%s exec=%s runs=%d avg_ms=%.1f min_ms=%d max_ms=%d\n", parts[1], parts[2], parts[3], count[k], avg, min[k], max[k]
    }
  }
' "${out_csv}"

echo "CSV written: ${out_csv}"

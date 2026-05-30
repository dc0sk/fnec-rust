#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/pi-benchmark-history.sh append <benchmark.csv> [history.csv]
  scripts/pi-benchmark-history.sh trend <history.csv>

Commands:
  append  Append one benchmark CSV into a persistent history CSV.
          If history.csv is omitted, defaults to benchmarks/pi-benchmark-history.csv.

  trend   Summarize trend per (deck, solver, exec_mode) from history CSV snapshots.
          Output columns:
            deck, solver, exec_mode, snapshots,
            first_avg_ms, latest_avg_ms, delta_pct,
            latest_timestamp_utc, latest_git_sha, latest_source_csv

History CSV schema:
  ingested_at_utc,git_sha,source_csv,<original benchmark CSV columns...>

Notes:
  - Only rows with status=ok are used by trend.
  - The benchmark CSV must be produced by scripts/pi-remote-benchmark.sh.
  - Supports both current header (`timestamp_utc,...,exec_mode,...`) and
    legacy local header (`timestamp_unix_ms,...,exec,...`).
EOF
}

default_history="benchmarks/pi-benchmark-history.csv"

if [[ ${1:-} == "-h" || ${1:-} == "--help" || $# -eq 0 ]]; then
  usage
  exit 0
fi

cmd="$1"
shift

case "${cmd}" in
  append)
    if [[ $# -lt 1 || $# -gt 2 ]]; then
      usage >&2
      exit 1
    fi

    in_csv="$1"
    history_csv="${2:-${default_history}}"

    if [[ ! -f "${in_csv}" ]]; then
      echo "error: benchmark CSV not found: ${in_csv}" >&2
      exit 1
    fi

    in_header="$(head -n 1 "${in_csv}")"
    current_header='timestamp_utc,target,deck,solver,run,status,elapsed_ms,diag_mode,pulse_rhs,freq_mhz,abs_res,rel_res,exec_mode,diag_spread,sin_rel_res'
    legacy_header='timestamp_unix_ms,target,deck,solver,run,status,elapsed_ms,diag_mode,pulse_rhs,exec,freq_mhz,abs_res,rel_res,diag_spread,sin_rel_res'
    format=""
    if [[ "${in_header}" == "${current_header}" ]]; then
      format="current"
    elif [[ "${in_header}" == "${legacy_header}" ]]; then
      format="legacy"
    else
      echo "error: unsupported benchmark CSV header in ${in_csv}" >&2
      echo "expected one of:" >&2
      echo "  ${current_header}" >&2
      echo "  ${legacy_header}" >&2
      exit 1
    fi

    mkdir -p "$(dirname "${history_csv}")"

    history_header='ingested_at_utc,git_sha,source_csv,timestamp_utc,target,deck,solver,run,status,elapsed_ms,diag_mode,pulse_rhs,freq_mhz,abs_res,rel_res,exec_mode,diag_spread,sin_rel_res'
    if [[ ! -f "${history_csv}" ]]; then
      printf '%s\n' "${history_header}" > "${history_csv}"
    else
      current_header="$(head -n 1 "${history_csv}")"
      if [[ "${current_header}" != "${history_header}" ]]; then
        echo "error: history CSV header mismatch in ${history_csv}" >&2
        echo "expected: ${history_header}" >&2
        echo "actual:   ${current_header}" >&2
        exit 1
      fi
    fi

    ingested_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    git_sha="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    source_name="${in_csv}"

    if [[ "${format}" == "current" ]]; then
      awk -F, -v ts="${ingested_at}" -v sha="${git_sha}" -v src="${source_name}" '
        NR == 1 { next }
        NF > 1 {
          printf "%s,%s,%s,%s\n", ts, sha, src, $0
        }
      ' "${in_csv}" >> "${history_csv}"
    else
      # legacy local format: exec column appears before freq/abs/rel and
      # timestamp is in unix-ms form; normalize into canonical field order.
      awk -F, -v ts="${ingested_at}" -v sha="${git_sha}" -v src="${source_name}" '
        NR == 1 { next }
        NF > 1 {
          printf "%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n",
            ts, sha, src,
            $1, $2, $3, $4, $5, $6, $7,
            $8, $9, $11, $12, $13, $10, $14, $15
        }
      ' "${in_csv}" >> "${history_csv}"
    fi

    echo "Appended benchmark rows from ${in_csv} to ${history_csv}"
    ;;

  trend)
    if [[ $# -ne 1 ]]; then
      usage >&2
      exit 1
    fi

    history_csv="$1"
    if [[ ! -f "${history_csv}" ]]; then
      echo "error: history CSV not found: ${history_csv}" >&2
      exit 1
    fi

    if ! head -n 1 "${history_csv}" | grep -q '^ingested_at_utc,git_sha,source_csv,timestamp_utc,target,deck,solver,run,status,elapsed_ms,diag_mode,pulse_rhs,freq_mhz,abs_res,rel_res,exec_mode,diag_spread,sin_rel_res$'; then
      echo "error: unsupported history CSV header in ${history_csv}" >&2
      exit 1
    fi

    tmp_agg="$(mktemp)"
    trap 'rm -f "${tmp_agg}"' EXIT

    # Aggregate one snapshot row per (source_csv, git_sha, deck, solver, exec_mode).
    awk -F, '
      NR == 1 { next }
      $9 != "ok" { next }
      {
        snapshot = $3 "|" $2 "|" $6 "|" $7 "|" $16
        count[snapshot] += 1
        sum[snapshot] += $10
        if (!(snapshot in min) || $10 < min[snapshot]) {
          min[snapshot] = $10
        }
        if (!(snapshot in max) || $10 > max[snapshot]) {
          max[snapshot] = $10
        }
        if (!(snapshot in latest_ts) || $4 > latest_ts[snapshot]) {
          latest_ts[snapshot] = $4
        }
      }
      END {
        for (k in count) {
          split(k, p, "|")
          avg = sum[k] / count[k]
          printf "%s\t%s\t%s\t%s\t%s\t%.6f\t%s\t%s\n", p[1], p[2], p[3], p[4], p[5], avg, latest_ts[k], k
        }
      }
    ' "${history_csv}" | sort -t $'\t' -k3,3 -k4,4 -k5,5 -k7,7 > "${tmp_agg}"

    {
      echo "deck,solver,exec_mode,snapshots,first_avg_ms,latest_avg_ms,delta_pct,latest_timestamp_utc,latest_git_sha,latest_source_csv"
      awk -F'\t' '
        {
          src = $1
          sha = $2
          deck = $3
          solver = $4
          exec_mode = $5
          avg = $6 + 0.0
          ts = $7
          combo = deck "|" solver "|" exec_mode

          if (!(combo in seen)) {
            seen[combo] = 1
            snaps[combo] = 1
            first[combo] = avg
            latest[combo] = avg
            latest_ts[combo] = ts
            latest_sha[combo] = sha
            latest_src[combo] = src
            next
          }

          snaps[combo] += 1
          latest[combo] = avg
          latest_ts[combo] = ts
          latest_sha[combo] = sha
          latest_src[combo] = src
        }
        END {
          for (combo in seen) {
            split(combo, p, "|")
            delta = "n/a"
            if (first[combo] != 0) {
              delta = sprintf("%+.2f%%", ((latest[combo] - first[combo]) / first[combo]) * 100.0)
            }
            printf "%s,%s,%s,%d,%.3f,%.3f,%s,%s,%s,%s\n", p[1], p[2], p[3], snaps[combo], first[combo], latest[combo], delta, latest_ts[combo], latest_sha[combo], latest_src[combo]
          }
        }
      ' "${tmp_agg}" | sort
    }
    ;;

  *)
    echo "error: unknown command: ${cmd}" >&2
    usage >&2
    exit 1
    ;;
esac
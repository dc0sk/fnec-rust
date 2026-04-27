#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/pi-benchmark-summary.sh <benchmark.csv>

Summarize a benchmark CSV produced by scripts/pi-remote-benchmark.sh.

Sections:
  1. Average elapsed_ms grouped by deck, solver, exec_mode
  2. Unique diag_mode values with counts
  3. Sinusoidal rows that fell back to another diag_mode
  4. sin_rel_res min/max by deck and exec_mode for solver=sinusoidal
  5. diag_spread min/max by deck and solver

Notes:
  - Rows with status != ok are ignored in aggregate summaries.
  - The fallback section only prints when mismatches exist.
EOF
}

format_table() {
  if command -v column >/dev/null 2>&1; then
    column -t -s $'\t'
  else
    cat
  fi
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 1
fi

csv="$1"
if [[ ! -f "${csv}" ]]; then
  echo "error: benchmark CSV not found: ${csv}" >&2
  exit 1
fi

echo "--- 1. Average elapsed_ms grouped by deck, solver, exec_mode ---"
awk -F, '
NR > 1 && $6 == "ok" {
  key = $3 "\t" $4 "\t" $13
  sum[key] += $7
  count[key] += 1
}
END {
  for (k in sum) {
    printf "%s\t%.4f\n", k, sum[k] / count[k]
  }
}
' "${csv}" | sort | format_table

echo
echo "--- 2. Unique diag_mode values present, with counts ---"
awk -F, '
NR > 1 {
  count[$8] += 1
}
END {
  for (mode in count) {
    printf "%s\t%d\n", mode, count[mode]
  }
}
' "${csv}" | sort | format_table

echo
echo "--- 3. Rows where solver=sinusoidal and diag_mode != sinusoidal ---"
fallback_rows="$(awk -F, 'NR > 1 && $4 == "sinusoidal" && $8 != "sinusoidal" { print $0 }' "${csv}")"
if [[ -n "${fallback_rows}" ]]; then
  printf '%s\n' "${fallback_rows}"
else
  echo "None found."
fi

echo
echo "--- 4. For solver=sinusoidal, summarize sin_rel_res min/max by deck and exec_mode ---"
awk -F, '
NR > 1 && $6 == "ok" && $4 == "sinusoidal" {
  key = $3 "\t" $13
  val = $15 + 0.0
  if (!(key in min) || val < min[key]) min[key] = val
  if (!(key in max) || val > max[key]) max[key] = val
}
END {
  for (k in min) {
    printf "%s\t%.6e\t%.6e\n", k, min[k], max[k]
  }
}
' "${csv}" | sort | format_table

echo
echo "--- 5. For all rows, summarize diag_spread min/max by deck and solver ---"
awk -F, '
NR > 1 && $6 == "ok" {
  key = $3 "\t" $4
  val = $14 + 0.0
  if (!(key in min) || val < min[key]) min[key] = val
  if (!(key in max) || val > max[key]) max[key] = val
}
END {
  for (k in min) {
    printf "%s\t%.6e\t%.6e\n", k, min[k], max[k]
  }
}
' "${csv}" | sort | format_table
#!/usr/bin/env bash
# Compare two benchmark JSON artifacts against configurable regression thresholds.
#
# Usage: scripts/benchmark-compare-json.sh [options] <baseline.json> <candidate.json>
#
# Options:
#   --gates-file <file>       TOML threshold config (default: .benchmark-gates.toml)
#   --max-regression-pct N    Override max_regression_pct threshold from gates file
#   --max-gpu-cpu-ratio R     Override max_gpu_cpu_ratio threshold from gates file
#   -h, --help
#
# Exit codes:
#   0  All gates passed
#   1  One or more gates failed
set -euo pipefail
export LC_NUMERIC=C   # ensure dot decimal separator in awk/printf across all locales

usage() {
    cat <<'EOF'
Usage: scripts/benchmark-compare-json.sh [options] <baseline.json> <candidate.json>

Compare benchmark JSON artifacts and fail if regression exceeds thresholds.

Options:
  --gates-file <file>       TOML threshold config (default: .benchmark-gates.toml)
  --max-regression-pct N    Override max regression threshold (percent)
  --max-gpu-cpu-ratio R     Override GPU/CPU-single ratio limit
  -h, --help                Show this help
EOF
}

gates_file=".benchmark-gates.toml"
max_pct_override=""
max_ratio_override=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --gates-file)
            shift; gates_file="$1" ;;
        --max-regression-pct)
            shift; max_pct_override="$1" ;;
        --max-gpu-cpu-ratio)
            shift; max_ratio_override="$1" ;;
        -h|--help)
            usage; exit 0 ;;
        --)
            shift; break ;;
        -*)
            echo "error: unknown option: $1" >&2; usage >&2; exit 1 ;;
        *)
            break ;;
    esac
    shift
done

if [[ $# -ne 2 ]]; then
    echo "error: expected exactly 2 positional arguments (baseline.json candidate.json)" >&2
    usage >&2
    exit 1
fi

baseline_file="$1"
candidate_file="$2"

# --- Read thresholds ---
read_toml_value() {
    local key="$1"
    local file="$2"
    local default="$3"
    if [[ -f "${file}" ]]; then
        local val
        val=$(grep -E "^\s*${key}\s*=" "${file}" 2>/dev/null \
              | tail -1 | sed 's/.*=\s*//' | tr -d '[:space:]"' || true)
        echo "${val:-${default}}"
    else
        echo "${default}"
    fi
}

if [[ -n "${max_pct_override}" ]]; then
    max_pct="${max_pct_override}"
else
    max_pct=$(read_toml_value "max_regression_pct" "${gates_file}" "50.0")
fi

if [[ -n "${max_ratio_override}" ]]; then
    max_gpu_cpu_ratio="${max_ratio_override}"
else
    max_gpu_cpu_ratio=$(read_toml_value "max_gpu_cpu_ratio" "${gates_file}" "1.25")
fi

echo "Benchmark regression gate"
echo "  baseline:           ${baseline_file}"
echo "  candidate:          ${candidate_file}"
echo "  max_regression_pct: ${max_pct}"
echo "  max_gpu_cpu_ratio:  ${max_gpu_cpu_ratio}"
echo ""

fail=0

# --- Per-(deck, solver, exec_mode) regression check ---
echo "=== Timing regression check ==="
while IFS= read -r entry; do
    deck=$(jq -r   '.deck'      <<< "${entry}")
    solver=$(jq -r '.solver'    <<< "${entry}")
    mode=$(jq -r   '.exec_mode' <<< "${entry}")
    cand_avg=$(jq -r '.avg_ms'  <<< "${entry}")

    base_avg=$(jq -r \
        --arg d "${deck}" --arg s "${solver}" --arg m "${mode}" \
        '.summary[] | select(.deck==$d and .solver==$s and .exec_mode==$m) | .avg_ms' \
        "${baseline_file}" 2>/dev/null | head -1 || true)

    if [[ -z "${base_avg}" || "${base_avg}" == "null" ]]; then
        printf "  SKIP  %-44s no baseline entry\n" "${deck}/${solver}/${mode}"
        continue
    fi

    # regression_pct = (cand - base) / base * 100
    regression_pct=$(awk "BEGIN {
        if (${base_avg} > 0)
            printf \"%.1f\", (${cand_avg} - ${base_avg}) / ${base_avg} * 100
        else
            print \"0.0\"
    }")

    if awk "BEGIN { exit (${regression_pct} > ${max_pct}) ? 0 : 1 }"; then
        printf "  FAIL  %-44s %sms vs baseline %sms  (+%s%% > %s%%)\n" \
            "${deck}/${solver}/${mode}" "${cand_avg}" "${base_avg}" \
            "${regression_pct}" "${max_pct}"
        fail=1
    else
        printf "  OK    %-44s %sms vs baseline %sms  (%+.1f%%)\n" \
            "${deck}/${solver}/${mode}" "${cand_avg}" "${base_avg}" \
            "${regression_pct}"
    fi
done < <(jq -c '.summary[]' "${candidate_file}")

# --- GPU vs CPU-single ratio check ---
echo ""
echo "=== GPU/CPU-single ratio check (limit: ${max_gpu_cpu_ratio}×) ==="
while IFS= read -r entry; do
    deck=$(jq -r   '.deck'   <<< "${entry}")
    solver=$(jq -r '.solver' <<< "${entry}")
    gpu_avg=$(jq -r '.avg_ms' <<< "${entry}")

    cpu_avg=$(jq -r \
        --arg d "${deck}" --arg s "${solver}" \
        '.summary[] | select(.deck==$d and .solver==$s and .exec_mode=="cpu-single") | .avg_ms' \
        "${candidate_file}" 2>/dev/null | head -1 || true)

    if [[ -z "${cpu_avg}" || "${cpu_avg}" == "null" || "${cpu_avg}" == "0" ]]; then
        continue
    fi

    ratio=$(awk "BEGIN { printf \"%.3f\", ${gpu_avg} / ${cpu_avg} }")

    if awk "BEGIN { exit (${ratio} > ${max_gpu_cpu_ratio}) ? 0 : 1 }"; then
        printf "  FAIL  %-34s GPU/CPU = %s > %s\n" \
            "${deck}/${solver}" "${ratio}" "${max_gpu_cpu_ratio}"
        fail=1
    else
        printf "  OK    %-34s GPU/CPU = %s (limit: %s)\n" \
            "${deck}/${solver}" "${ratio}" "${max_gpu_cpu_ratio}"
    fi
done < <(jq -c '.summary[] | select(.exec_mode=="gpu")' "${candidate_file}")

echo ""
if [[ ${fail} -ne 0 ]]; then
    echo "Result: FAILED"
    exit 1
fi
echo "Result: PASSED"

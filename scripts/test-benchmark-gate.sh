#!/usr/bin/env bash
# Test that benchmark-compare-json.sh correctly fires on an injected regression
# and passes on identical inputs.
#
# Usage: scripts/test-benchmark-gate.sh
# Exit code: 0 on all pass, 1 on any failure.
set -euo pipefail

tmpdir=$(mktemp -d)
trap 'rm -rf "${tmpdir}"' EXIT

compare="scripts/benchmark-compare-json.sh"

if [[ ! -f "${compare}" ]]; then
    echo "error: ${compare} not found; run from workspace root" >&2
    exit 1
fi

baseline="${tmpdir}/baseline.json"
candidate_ok="${tmpdir}/candidate-ok.json"
candidate_bad="${tmpdir}/candidate-bad.json"

# Baseline: dipole-freesp-51seg, hallen, cpu-single, avg=20ms
cat > "${baseline}" <<'EOF'
{
  "schema_version": "1",
  "generated_at": "2026-01-01T00:00:00Z",
  "git_sha": "aaaaaa",
  "runner_nproc": 4,
  "runs": [],
  "summary": [
    {
      "deck": "dipole-freesp-51seg",
      "solver": "hallen",
      "exec_mode": "cpu-single",
      "n_runs": 3,
      "avg_ms": 20,
      "min_ms": 19,
      "max_ms": 21
    },
    {
      "deck": "dipole-freesp-51seg",
      "solver": "hallen",
      "exec_mode": "gpu",
      "n_runs": 3,
      "avg_ms": 22,
      "min_ms": 21,
      "max_ms": 23
    }
  ]
}
EOF

# Good candidate: avg=22ms (+10% vs baseline of 20ms) — should pass 50% gate
cat > "${candidate_ok}" <<'EOF'
{
  "schema_version": "1",
  "generated_at": "2026-01-02T00:00:00Z",
  "git_sha": "bbbbbb",
  "runner_nproc": 4,
  "runs": [],
  "summary": [
    {
      "deck": "dipole-freesp-51seg",
      "solver": "hallen",
      "exec_mode": "cpu-single",
      "n_runs": 3,
      "avg_ms": 22,
      "min_ms": 21,
      "max_ms": 23
    },
    {
      "deck": "dipole-freesp-51seg",
      "solver": "hallen",
      "exec_mode": "gpu",
      "n_runs": 3,
      "avg_ms": 24,
      "min_ms": 23,
      "max_ms": 25
    }
  ]
}
EOF

# Bad candidate: avg=60ms (+200% regression) — should fail 50% gate
cat > "${candidate_bad}" <<'EOF'
{
  "schema_version": "1",
  "generated_at": "2026-01-03T00:00:00Z",
  "git_sha": "cccccc",
  "runner_nproc": 4,
  "runs": [],
  "summary": [
    {
      "deck": "dipole-freesp-51seg",
      "solver": "hallen",
      "exec_mode": "cpu-single",
      "n_runs": 3,
      "avg_ms": 60,
      "min_ms": 58,
      "max_ms": 62
    },
    {
      "deck": "dipole-freesp-51seg",
      "solver": "hallen",
      "exec_mode": "gpu",
      "n_runs": 3,
      "avg_ms": 65,
      "min_ms": 63,
      "max_ms": 67
    }
  ]
}
EOF

passed=0
failed=0

run_test() {
    local desc="$1"
    local expect_fail="$2"   # 1 = expect non-zero exit, 0 = expect zero exit
    shift 2

    if [[ "${expect_fail}" -eq 1 ]]; then
        if bash "${compare}" "$@" > "${tmpdir}/out.txt" 2>&1; then
            echo "FAIL  ${desc}: expected non-zero exit but compare passed"
            cat "${tmpdir}/out.txt"
            ((failed++)) || true
        else
            echo "OK    ${desc}: gate fired as expected (exit non-zero)"
            ((passed++)) || true
        fi
    else
        if bash "${compare}" "$@" > "${tmpdir}/out.txt" 2>&1; then
            echo "OK    ${desc}: gate passed as expected"
            ((passed++)) || true
        else
            echo "FAIL  ${desc}: expected zero exit but compare failed"
            cat "${tmpdir}/out.txt"
            ((failed++)) || true
        fi
    fi
}

echo "=== Benchmark gate injection tests ==="

# Test 1: identical files should pass
run_test "identical baseline vs baseline" 0 \
    --max-regression-pct 50 "${baseline}" "${baseline}"

# Test 2: +10% regression should pass 50% gate
run_test "+10% regression passes 50% gate" 0 \
    --max-regression-pct 50 "${baseline}" "${candidate_ok}"

# Test 3: +200% regression should fail 50% gate
run_test "+200% regression fails 50% gate" 1 \
    --max-regression-pct 50 "${baseline}" "${candidate_bad}"

# Test 4: +200% regression passes a 300% gate (gate set higher)
run_test "+200% regression passes 300% gate" 0 \
    --max-regression-pct 300 "${baseline}" "${candidate_bad}"

# Test 5: GPU/CPU ratio check — candidate_ok gpu=24ms, cpu=22ms → ratio 1.09 < 1.25 → pass
run_test "GPU/CPU ratio 1.09 passes 1.25 limit" 0 \
    --max-regression-pct 50 --max-gpu-cpu-ratio 1.25 "${baseline}" "${candidate_ok}"

# Test 6: GPU/CPU ratio check with tight limit — ratio 1.09 > 1.05 → fail
run_test "GPU/CPU ratio 1.09 fails 1.05 limit" 1 \
    --max-regression-pct 50 --max-gpu-cpu-ratio 1.05 "${baseline}" "${candidate_ok}"

# --- FND-351: a gate that compared nothing must not pass ---
printf 'not json\n' > "${tmpdir}/not-json.json"
printf '{"summary":[]}\n' > "${tmpdir}/empty-summary.json"
printf '{"runs":[]}\n' > "${tmpdir}/no-summary.json"
jq '.summary |= map(.deck = "other-deck")' "${baseline}" > "${tmpdir}/unmatched.json"

run_test "candidate that is not JSON is refused" 1 \
    "${baseline}" "${tmpdir}/not-json.json"
run_test "missing candidate file is refused" 1 \
    "${baseline}" "${tmpdir}/does-not-exist.json"
run_test "candidate with an empty summary is refused" 1 \
    "${baseline}" "${tmpdir}/empty-summary.json"
run_test "candidate with no summary is refused" 1 \
    "${baseline}" "${tmpdir}/no-summary.json"
run_test "baseline that is not JSON is refused" 1 \
    "${tmpdir}/not-json.json" "${baseline}"
run_test "candidate sharing no entry with the baseline is refused" 1 \
    "${baseline}" "${tmpdir}/unmatched.json"

# --- pi-benchmark-compare.sh (CSV): the same rule, exit 3 ---
pi="scripts/pi-benchmark-compare.sh"
hdr="timestamp_utc,target,deck,solver,run,status,elapsed_ms,diag_mode,pulse_rhs,freq_mhz,abs_res,rel_res,exec_mode,diag_spread,sin_rel_res"
row() { # deck exec status ms
    printf 't,host,%s,hallen,1,%s,%s,dense,1,14,1e-9,1e-9,%s,0,0\n' "$1" "$3" "$4" "$2"
}
{ echo "${hdr}"; } > "${tmpdir}/empty.csv"
{ echo "${hdr}"; row d gpu error 10; row d cpu-single ok 10; } > "${tmpdir}/gpu-all-error.csv"
{ echo "${hdr}"; row d gpu ok 11; row d cpu-single ok 10; } > "${tmpdir}/gpu-ok.csv"
{ echo "${hdr}"; row d gpu ok 20; row d cpu-single ok 10; } > "${tmpdir}/gpu-slow.csv"

pi_test() { # desc expected-exit args...
    local desc="$1" want="$2" got=0
    shift 2
    bash "${pi}" "$@" > "${tmpdir}/out.txt" 2>&1 || got=$?
    if [[ "${got}" -eq "${want}" ]]; then
        echo "OK    ${desc}: exit ${got}"
        ((passed++)) || true
    else
        echo "FAIL  ${desc}: expected exit ${want}, got ${got}"
        cat "${tmpdir}/out.txt"
        ((failed++)) || true
    fi
}
pi_test "G5: GPU 10% slower passes 25%" 0 --gpu-vs-cpu-max-pct 25 "${tmpdir}/gpu-ok.csv"
pi_test "G5: GPU 100% slower fails 25%" 2 --gpu-vs-cpu-max-pct 25 "${tmpdir}/gpu-slow.csv"
pi_test "G5: empty CSV compares nothing" 3 --gpu-vs-cpu-max-pct 25 "${tmpdir}/empty.csv"
pi_test "G5: all-error GPU rows compare nothing" 3 --gpu-vs-cpu-max-pct 25 "${tmpdir}/gpu-all-error.csv"
pi_test "two-file: same CSV passes" 0 --max-delta-pct 15 --fail-on-mode-drift "${tmpdir}/gpu-ok.csv" "${tmpdir}/gpu-ok.csv"
pi_test "two-file: +100% fails 15%" 2 --max-delta-pct 15 "${tmpdir}/gpu-ok.csv" "${tmpdir}/gpu-slow.csv"
pi_test "two-file: two empty CSVs compare nothing" 3 --max-delta-pct 15 --fail-on-mode-drift "${tmpdir}/empty.csv" "${tmpdir}/empty.csv"

echo ""
echo "Results: ${passed} passed, ${failed} failed"

if [[ ${failed} -ne 0 ]]; then
    exit 1
fi

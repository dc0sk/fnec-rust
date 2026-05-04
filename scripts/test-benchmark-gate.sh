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

echo ""
echo "Results: ${passed} passed, ${failed} failed"

if [[ ${failed} -ne 0 ]]; then
    exit 1
fi

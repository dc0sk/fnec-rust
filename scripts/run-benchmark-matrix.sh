#!/usr/bin/env bash
# Run the three-mode benchmark matrix and emit a JSON artifact.
#
# Usage: scripts/run-benchmark-matrix.sh [output-file]
#   output-file  Path for the JSON result (default: benchmark-result.json)
#
# Environment overrides:
#   FNEC_BINARY         Path to fnec binary (default: ./target/release/fnec)
#   FNEC_BENCH_DECKS    Space-separated deck paths (default: two small corpus decks)
#   FNEC_BENCH_SOLVERS  Space-separated solver names (default: hallen pulse)
#   FNEC_BENCH_RUNS     Runs per combination (default: 3)
#   FNEC_BENCH_MODES    Space-separated modes: cpu-single cpu-multi gpu (default: all three)
set -euo pipefail
export LC_NUMERIC=C

out="${1:-benchmark-result.json}"
binary="${FNEC_BINARY:-./target/release/fnec}"
decks="${FNEC_BENCH_DECKS:-corpus/dipole-freesp-51seg.nec corpus/dipole-ground-51seg.nec}"
solvers="${FNEC_BENCH_SOLVERS:-hallen pulse}"
n_runs="${FNEC_BENCH_RUNS:-3}"
modes="${FNEC_BENCH_MODES:-cpu-single cpu-multi gpu}"

if [[ ! -x "${binary}" ]]; then
    echo "error: benchmark binary not found or not executable: ${binary}" >&2
    echo "Run: cargo build --release -p nec-cli" >&2
    exit 1
fi

# Portable nproc
nproc_val="$(nproc 2>/dev/null || sysctl -n hw.logicalcpu 2>/dev/null || echo 2)"
git_sha="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Arrays to accumulate JSON objects (no jq required for building)
declare -a run_entries=()
declare -a summary_entries=()

for deck_path in ${decks}; do
    deck_name="$(basename "${deck_path}" .nec)"

    for solver in ${solvers}; do
        for exec_mode in ${modes}; do
            # Configure environment for this mode
            case "${exec_mode}" in
                cpu-single)
                    export RAYON_NUM_THREADS=1
                    export FNEC_ACCEL_STUB_GPU=0
                    exec_arg="cpu"
                    ;;
                cpu-multi)
                    export RAYON_NUM_THREADS="${nproc_val}"
                    export FNEC_ACCEL_STUB_GPU=0
                    exec_arg="hybrid"
                    ;;
                gpu)
                    export RAYON_NUM_THREADS="${nproc_val}"
                    export FNEC_ACCEL_STUB_GPU=1
                    exec_arg="gpu"
                    ;;
                *)
                    echo "error: unknown exec_mode: ${exec_mode}" >&2
                    exit 1
                    ;;
            esac

            echo -n "  ${deck_name}/${solver}/${exec_mode}: "

            times_ms=()
            for (( i=0; i<n_runs; i++ )); do
                t0=$(date +%s%N)
                "${binary}" --solver "${solver}" --exec "${exec_arg}" "${deck_path}" \
                    > /dev/null 2>&1 || true
                t1=$(date +%s%N)
                elapsed_ms=$(( (t1 - t0) / 1000000 ))
                times_ms+=("${elapsed_ms}")
                echo -n "${elapsed_ms}ms "
                run_entries+=(
                    "{\"deck\":\"${deck_name}\",\"solver\":\"${solver}\",\"exec_mode\":\"${exec_mode}\",\"elapsed_ms\":${elapsed_ms}}"
                )
            done
            echo ""

            # Compute min/max/avg using awk (portable, no bc needed)
            stats=$(printf '%s\n' "${times_ms[@]}" | awk '
                BEGIN { min=999999999; max=0; total=0; n=0 }
                { total+=$1; n++; if ($1<min) min=$1; if ($1>max) max=$1 }
                END { printf "%d %d %d", int(total/n), min, max }
            ')
            avg_ms=$(echo "${stats}" | cut -d' ' -f1)
            min_ms=$(echo "${stats}" | cut -d' ' -f2)
            max_ms=$(echo "${stats}" | cut -d' ' -f3)

            summary_entries+=(
                "{\"deck\":\"${deck_name}\",\"solver\":\"${solver}\",\"exec_mode\":\"${exec_mode}\",\"n_runs\":${n_runs},\"avg_ms\":${avg_ms},\"min_ms\":${min_ms},\"max_ms\":${max_ms}}"
            )
        done
    done
done

# Assemble JSON output using jq for well-formed output
runs_array="[$(IFS=,; echo "${run_entries[*]}")]"
summary_array="[$(IFS=,; echo "${summary_entries[*]}")]"

jq -n \
    --arg schema_version "1" \
    --arg generated_at "${generated_at}" \
    --arg git_sha "${git_sha}" \
    --argjson runner_nproc "${nproc_val}" \
    --argjson runs "${runs_array}" \
    --argjson summary "${summary_array}" \
    '{
        schema_version: $schema_version,
        generated_at: $generated_at,
        git_sha: $git_sha,
        runner_nproc: $runner_nproc,
        runs: $runs,
        summary: $summary
    }' > "${out}"

echo ""
echo "Benchmark matrix complete. Artifact written: ${out}"
echo "Summary:"
jq -r '.summary[] | "  \(.exec_mode)\t\(.solver)\t\(.deck)\tavg=\(.avg_ms)ms  [min=\(.min_ms) max=\(.max_ms)]"' "${out}"

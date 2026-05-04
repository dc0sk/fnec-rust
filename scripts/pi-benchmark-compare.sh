#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF_USAGE'
Usage: scripts/pi-benchmark-compare.sh [options] <base.csv> <candidate.csv>
       scripts/pi-benchmark-compare.sh --gpu-vs-cpu-max-pct <number> <candidate.csv>

Compare two benchmark CSV files produced by scripts/pi-remote-benchmark.sh and
print per deck+solver deltas for timing and residual diagnostics.

In the two-file form, base.csv is compared to candidate.csv.

In the single-file form (--gpu-vs-cpu-max-pct), GPU rows are compared to CPU
rows within candidate.csv for the same (deck, solver) combination.  Fails if
any GPU average exceeds the CPU average by more than the given percentage.
This is the G5 gate (PH5-CHK-005): GPU ≥ 0.8× CPU (limit: 25%).

Columns in output (two-file mode):
    deck, solver, exec, base_runs, cand_runs, base_avg_ms, cand_avg_ms, delta_ms, delta_pct,
    base_mode, cand_mode, base_abs_res, cand_abs_res, abs_res_ratio,
    base_rel_res, cand_rel_res, rel_res_ratio,
    base_diag_spread, cand_diag_spread, diag_spread_ratio,
    base_sin_rel_res, cand_sin_rel_res, sin_rel_res_ratio

Columns in output (single-file mode):
    deck, solver, cpu_runs, gpu_runs, cpu_avg_ms, gpu_avg_ms, ratio

Notes:
    - Rows with status != ok are ignored.
    - Ratio columns are candidate/base (two-file) or gpu/cpu (single-file).
    - If a base value is zero, percentage/ratio fields are reported as n/a.

Options:
  --max-delta-pct <number>      Fail if candidate timing regression exceeds this percentage.
                                Only positive (slower) regressions are checked.
  --fail-on-mode-drift          Fail if candidate diag_mode differs from base diag_mode.
  --gpu-vs-cpu-max-pct <number> Single-file mode: fail if GPU avg exceeds CPU avg by more
                                than this percentage for any (deck, solver) combo.
                                Use 25 for the G5 gate (GPU ≥ 0.8× CPU).
  -h, --help                    Show this help text.
EOF_USAGE
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
    usage
    exit 0
fi

max_delta_pct=""
fail_on_mode_drift=0
gpu_vs_cpu_max_pct=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --max-delta-pct)
            shift
            if [[ $# -eq 0 ]]; then
                echo "error: --max-delta-pct requires a numeric value" >&2
                exit 1
            fi
            max_delta_pct="$1"
            ;;
        --fail-on-mode-drift)
            fail_on_mode_drift=1
            ;;
        --gpu-vs-cpu-max-pct)
            shift
            if [[ $# -eq 0 ]]; then
                echo "error: --gpu-vs-cpu-max-pct requires a numeric value" >&2
                exit 1
            fi
            gpu_vs_cpu_max_pct="$1"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --)
            shift
            break
            ;;
        -*)
            echo "error: unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
        *)
            break
            ;;
    esac
    shift
done

if [[ -n "${gpu_vs_cpu_max_pct}" ]]; then
    if [[ $# -ne 1 ]]; then
        echo "error: --gpu-vs-cpu-max-pct requires exactly one CSV file argument" >&2
        usage >&2
        exit 1
    fi
else
    if [[ $# -ne 2 ]]; then
        usage >&2
        exit 1
    fi
fi

if [[ -n "${max_delta_pct}" ]] && ! [[ "${max_delta_pct}" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    echo "error: --max-delta-pct must be a non-negative number" >&2
    exit 1
fi
if [[ -n "${gpu_vs_cpu_max_pct}" ]] && ! [[ "${gpu_vs_cpu_max_pct}" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    echo "error: --gpu-vs-cpu-max-pct must be a non-negative number" >&2
    exit 1
fi

if [[ -n "${gpu_vs_cpu_max_pct}" ]]; then
    # --- Single-file mode: GPU-vs-CPU comparison (gate G5) ---
    cand_csv="$1"
    if [[ ! -f "${cand_csv}" ]]; then
        echo "error: file not found: ${cand_csv}" >&2
        exit 1
    fi

    awk -F, -v max_pct="${gpu_vs_cpu_max_pct}" '
    FNR == 1 { next }
    $6 != "ok" { next }
    {
        ex = (NF >= 13 && $13 != "") ? $13 : "cpu"
        k = $3 "|" $4
        if (ex == "cpu") { cpu_c[k]++; cpu_t[k] += $7 }
        else if (ex == "gpu") { gpu_c[k]++; gpu_t[k] += $7 }
    }
    END {
        fail = 0
        for (k in cpu_c) {
            if (!(k in gpu_c)) continue
            if (cpu_c[k] == 0 || gpu_c[k] == 0) continue
            cpu_avg = cpu_t[k] / cpu_c[k]
            gpu_avg = gpu_t[k] / gpu_c[k]
            r = (cpu_avg > 0) ? gpu_avg / cpu_avg : 0
            split(k, p, "|")
            printf "%s,%s,%d,%d,%.3f,%.3f,%.4f\n", p[1], p[2], cpu_c[k], gpu_c[k], cpu_avg, gpu_avg, r
            if (max_pct != "" && cpu_avg > 0) {
                delta_pct = (gpu_avg - cpu_avg) / cpu_avg * 100.0
                if (delta_pct > (max_pct + 0.0)) {
                    printf "G5 violation: deck=%s solver=%s gpu_avg_ms=%.3f cpu_avg_ms=%.3f ratio=%.4f exceeds limit %.0f%%\n", p[1], p[2], gpu_avg, cpu_avg, r, max_pct > "/dev/stderr"
                    fail = 1
                }
            }
        }
        if (fail) exit 2
    }
    ' "${cand_csv}" | {
        echo "deck,solver,cpu_runs,gpu_runs,cpu_avg_ms,gpu_avg_ms,ratio"
        sort
    }

else
    # --- Two-file mode: baseline vs candidate comparison ---
    base_csv="$1"
    cand_csv="$2"

    if [[ ! -f "${base_csv}" ]]; then
        echo "error: base file not found: ${base_csv}" >&2
        exit 1
    fi
    if [[ ! -f "${cand_csv}" ]]; then
        echo "error: candidate file not found: ${cand_csv}" >&2
        exit 1
    fi

    awk -F, -v max_delta_pct="${max_delta_pct}" -v fail_mode_drift="${fail_on_mode_drift}" '
    function pct(delta, base) {
        if (base == 0) return "n/a"
        return sprintf("%+.2f%%", (delta / base) * 100.0)
    }
    function ratio(cand, base) {
        if (base == 0) return "n/a"
        return sprintf("%.6g", cand / base)
    }
    function m_merge(curr, nxt) {
        if (curr == "") return nxt
        if (curr == nxt) return curr
        return "mixed"
    }

    NR == FNR {
        if (FNR == 1 || $6 != "ok") next
        ex = (NF >= 13 && $13 != "") ? $13 : "cpu"
        ds = (NF >= 14 && $14 != "") ? $14 : 0
        sr = (NF >= 15 && $15 != "") ? $15 : 0
        k = $3 "|" $4 "|" ex
        all[k] = 1
        b_c[k] += 1
        b_t[k] += $7
        b_a[k] += $11
        b_r[k] += $12
        b_d[k] += ds
        b_s[k] += sr
        b_m[k] = m_merge(b_m[k], $8)
        next
    }
    {
        if (FNR == 1 || $6 != "ok") next
        ex = (NF >= 13 && $13 != "") ? $13 : "cpu"
        ds = (NF >= 14 && $14 != "") ? $14 : 0
        sr = (NF >= 15 && $15 != "") ? $15 : 0
        k = $3 "|" $4 "|" ex
        all[k] = 1
        c_c[k] += 1
        c_t[k] += $7
        c_a[k] += $11
        c_r[k] += $12
        c_d[k] += ds
        c_s[k] += sr
        c_m[k] = m_merge(c_m[k], $8)
    }
    END {
        fail = 0
        for (k in all) {
            split(k, p, "|")
            br = b_c[k]+0; cr = c_c[k]+0
            ba = (br>0)?b_t[k]/br:0; ca = (cr>0)?c_t[k]/cr:0
            babs = (br>0)?b_a[k]/br:0; cabs = (cr>0)?c_a[k]/cr:0
            brel = (br>0)?b_r[k]/br:0; crel = (cr>0)?c_r[k]/cr:0
            bdiag = (br>0)?b_d[k]/br:0; cdiag = (cr>0)?c_d[k]/cr:0
            bsin = (br>0)?b_s[k]/br:0; csin = (cr>0)?c_s[k]/cr:0
            bm = (br>0)?b_m[k]:"missing"; cm = (cr>0)?c_m[k]:"missing"

            if (br > 0 && cr > 0 && ba > 0) {
                delta_pct_num = ((ca - ba) / ba) * 100.0
                if (max_delta_pct != "" && delta_pct_num > (max_delta_pct + 0.0)) {
                    printf "threshold violation: deck=%s solver=%s exec=%s delta_pct=%+.2f%% exceeds max %s%%\n", p[1], p[2], p[3], delta_pct_num, max_delta_pct > "/dev/stderr"
                    fail = 1
                }
            }
            if (fail_mode_drift == "1" && bm != cm) {
                printf "mode drift violation: deck=%s solver=%s exec=%s base_mode=%s cand_mode=%s\n", p[1], p[2], p[3], bm, cm > "/dev/stderr"
                fail = 1
            }

            printf "%s,%s,%s,%d,%d,%.3f,%.3f,%+.3f,%s,%s,%s,%.6e,%.6e,%s,%.6e,%.6e,%s,%.6e,%.6e,%s,%.6e,%.6e,%s\n",
                p[1],p[2],p[3],br,cr,ba,ca,ca-ba,pct(ca-ba,ba),bm,cm,babs,cabs,ratio(cabs,babs),brel,crel,ratio(crel,brel),bdiag,cdiag,ratio(cdiag,bdiag),bsin,csin,ratio(csin,bsin)
        }
        if (fail) {
            exit 2
        }
    }
    ' "$base_csv" "$cand_csv" | {
        echo "deck,solver,exec,base_runs,cand_runs,base_avg_ms,cand_avg_ms,delta_ms,delta_pct,base_mode,cand_mode,base_abs_res,cand_abs_res,abs_res_ratio,base_rel_res,cand_rel_res,rel_res_ratio,base_diag_spread,cand_diag_spread,diag_spread_ratio,base_sin_rel_res,cand_sin_rel_res,sin_rel_res_ratio"
        sort
    }
fi

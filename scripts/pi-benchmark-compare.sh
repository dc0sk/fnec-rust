#!/usr/bin/env bash
set -euo pipefail

usage() {
        cat <<'EOF_USAGE'
Usage: scripts/pi-benchmark-compare.sh <base.csv> <candidate.csv>

Compare two benchmark CSV files produced by scripts/pi-remote-benchmark.sh and
print per deck+solver deltas for timing and residual diagnostics.

Columns in output:
    deck, solver, base_runs, cand_runs, base_avg_ms, cand_avg_ms, delta_ms, delta_pct,
    base_mode, cand_mode, base_abs_res, cand_abs_res, abs_res_ratio,
    base_rel_res, cand_rel_res, rel_res_ratio

Notes:
    - Rows with status != ok are ignored.
    - Ratio columns are candidate/base.
    - If a base value is zero, percentage/ratio fields are reported as n/a.
EOF_USAGE
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
    usage
    exit 0
fi

if [[ $# -ne 2 ]]; then
    usage >&2
    exit 1
fi

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

awk -F, '
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
    k = $3 "|" $4
    all[k] = 1
    b_c[k] += 1
    b_t[k] += $7
    b_a[k] += $11
    b_r[k] += $12
    b_m[k] = m_merge(b_m[k], $8)
    next
}
{
    if (FNR == 1 || $6 != "ok") next
    k = $3 "|" $4
    all[k] = 1
    c_c[k] += 1
    c_t[k] += $7
    c_a[k] += $11
    c_r[k] += $12
    c_m[k] = m_merge(c_m[k], $8)
}
END {
    for (k in all) {
        split(k, p, "|")
        br = b_c[k]+0; cr = c_c[k]+0
        ba = (br>0)?b_t[k]/br:0; ca = (cr>0)?c_t[k]/cr:0
        babs = (br>0)?b_a[k]/br:0; cabs = (cr>0)?c_a[k]/cr:0
        brel = (br>0)?b_r[k]/br:0; crel = (cr>0)?c_r[k]/cr:0
        bm = (br>0)?b_m[k]:"missing"; cm = (cr>0)?c_m[k]:"missing"
        
            printf "%s,%s,%d,%d,%.3f,%.3f,%+.3f,%s,%s,%s,%.6e,%.6e,%s,%.6e,%.6e,%s\n",
            p[1],p[2],br,cr,ba,ca,ca-ba,pct(ca-ba,ba),bm,cm,babs,cabs,ratio(cabs,babs),brel,crel,ratio(crel,brel)
    }
}
' "$base_csv" "$cand_csv" | {
    echo "deck,solver,base_runs,cand_runs,base_avg_ms,cand_avg_ms,delta_ms,delta_pct,base_mode,cand_mode,base_abs_res,cand_abs_res,abs_res_ratio,base_rel_res,cand_rel_res,rel_res_ratio"
    sort
}

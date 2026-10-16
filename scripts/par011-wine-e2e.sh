#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# End-to-end check of fnec as 4nec2's kernel, in a real 4nec2 under Wine
# (PAR-011 P3). Manual, not CI: it needs Wine, a 4nec2 install, Xvfb and xdotool.
#
#   scripts/par011-wine-e2e.sh KERNEL_DIR [DECK...]
#
# KERNEL_DIR is what scripts/build-windows-kernel.sh wrote. DECK names are the
# fixture decks in apps/nec-cli/tests/fixtures/par011 (default: the 7 printable
# ones and nt_net). For each, this:
#   1. backs up 4nec2's six kernels (exe/backup-<time>/, sha256 MANIFEST), as
#      docs/par011-dropin-contract.md's procedure says, and installs fnec under all
#      six names (4nec2 picks its kernel by segment count);
#   2. opens the deck in 4nec2 (copied to models/zzpar011/), runs Calculate → NEC
#      output-data → Use original file → Generate, by clicking at V5.9.3's
#      positions on a 1600x1200 Xvfb display;
#   3. reads what 4nec2 left in out/: a printable deck must end in RUN TIME and
#      carry fnec's banner, a refused one (nt_net) must hold fnec's reason and no
#      RUN TIME — the oracle is the file 4nec2 reads, not a screenshot;
#   4. restores the six kernels and checks them against the manifest.
# A screenshot of each run is kept beside the results for evidence.
#
# 4nec2 skips Generate when the deck's .out is already newer than the deck, and
# shows the old result: the deck's .inp and .out (and its DOS 8.3 name's) are
# deleted before each run, so a stale file cannot pass for a fresh one.
set -euo pipefail

kernels="${1:?usage: $0 KERNEL_DIR [DECK...]}"
shift
decks=("$@")
[[ ${#decks[@]} -gt 0 ]] || decks=(dip_free dip_ld dip_pec dip_gn0 dip_sweep two_src gm_gx nf nt_net)
repo="$(git rev-parse --show-toplevel)"
fixtures="$repo/apps/nec-cli/tests/fixtures/par011"
nec2="${FOURNEC2_DIR:-$HOME/.wine/drive_c/4nec2}"
exe="$nec2/exe"
outdir="$nec2/out"
results="${E2E_RESULTS:-$HOME/.cache/fnec-par011-e2e/$(date +%Y%m%d-%H%M%S)}"
names=(nec2dxs500 nec2dxs1K5 nec2dxs3k0 nec2dxs5k0 nec2dxs8k0 nec2dxs11k)
for t in wine Xvfb xdotool import; do command -v "$t" >/dev/null || { echo "$t is needed" >&2; exit 1; }; done
mkdir -p "$results" "$nec2/models/zzpar011"

export DISPLAY=:99
xvfb=""
if ! xdpyinfo >/dev/null 2>&1; then
    Xvfb :99 -screen 0 1600x1200x24 > "$results/xvfb.log" 2>&1 &
    xvfb=$!
    sleep 2
fi

backup="$exe/backup-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$backup"
for n in "${names[@]}"; do cp "$exe/$n.exe" "$backup/"; done
(cd "$backup" && sha256sum "${names[@]/%/.exe}" > MANIFEST.sha256)
restore() {
    for n in "${names[@]}"; do cp "$backup/$n.exe" "$exe/$n.exe"; done
    (cd "$exe" && sha256sum -c "$backup/MANIFEST.sha256" --quiet) &&
        echo "kernels restored and verified against $backup/MANIFEST.sha256"
}
# Kill only an Xvfb this started: `kill 0` would signal the whole process group.
trap 'restore; [[ -n "$xvfb" ]] && kill "$xvfb" 2>/dev/null; true' EXIT
for n in "${names[@]}"; do cp "$kernels/$n.exe" "$exe/$n.exe"; done

fail=0
for d in "${decks[@]}"; do
    # The deck as 4nec2 opens it: the fixture's .inp plus EN.
    { tr -d '\r' < "$fixtures/$d.inp"; echo EN; } | sed 's/$/\r/' > "$nec2/models/zzpar011/$d.nec"
    lc="${d,,}"
    for f in "$lc" "${lc:0:8}"; do rm -f "${outdir:?}/${f:?}.inp" "${outdir:?}/${f:?}.out"; done
    pkill -x 4nec2.exe || true
    sleep 2
    (cd "$exe" && wine 4nec2.exe "C:\\4nec2\\models\\zzpar011\\$d.nec" > "$results/$d.wine.log" 2>&1 &)
    sleep 9
    xdotool mousemove 213 80 click 1; sleep 1        # Calculate
    xdotool mousemove 245 100 click 1; sleep 2       # NEC output-data
    xdotool mousemove 418 268 click 1; sleep 0.5     # Use original file
    xdotool mousemove 440 404 click 1; sleep 15      # Generate
    import -window root "$results/$d.png"
    out="$outdir/$lc.out"
    if [[ ! -f "$out" ]]; then
        echo "$d: FAIL — no output file"; fail=1
    elif [[ "$d" == nt_net ]]; then
        if grep -q "FNEC: " "$out" && ! grep -q "RUN TIME" "$out"; then
            echo "$d: ok — refused, the kernel's failure written"
        else
            echo "$d: FAIL — a refused deck must leave fnec's reason and no RUN TIME"; fail=1
        fi
    elif grep -q "SOLVED BY FNEC" "$out" && tr -d '\r' < "$out" | grep -v '^ *$' | tail -1 | grep -q "^ RUN TIME"; then
        echo "$d: ok — solved by fnec, RUN TIME written"
    else
        echo "$d: FAIL — no fnec banner or no RUN TIME (another kernel ran, or fnec failed)"; fail=1
    fi
    cp "$out" "$results/" 2>/dev/null || true
done
pkill -x 4nec2.exe || true
echo "results: $results"
exit "$fail"

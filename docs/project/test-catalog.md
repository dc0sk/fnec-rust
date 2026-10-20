---
project: fnec-rust
doc: docs/project/test-catalog.md
status: living
last_updated: 2026-10-19
---

# Test catalog

The **tests layer**: every test file, its function count, what it validates, and
the checklist/requirement it gates. Counts are `#[test]`/`#[tokio::test]` function
counts (measured, not estimated). Aggregate pass/fail is recorded separately in
[test-results.md](test-results.md).

## Integration / contract tests

<!-- CHECKED: `scripts/check-test-catalog-counts.py` requires a row for every
     integration test binary the harness runs, keyed by its repo-relative source
     file, with its measured count, and no row for a file that runs nothing
     (FND-143). Two packages ship `tests/current_source_junction.rs`; they are
     two rows. New rows take "Validates" from the file's own header comment. -->

| Test file | # | Validates | Gates |
|:----------|:--|:----------|:------|
| `apps/nec-cli/tests/average_power_gain.rs` | 2 | FND-262: the average power gain is NEC-2's. On the 8 PAR-011 fixtures whose kernel printed an average, the kernel's own gains through `nec_solver::average_power_gain` give its printed average within 1e-3, and fnec's own average is within 0.5 % of it (the GN 2 deck is excepted). A single cut has no average, and isotropic 0 dBi averages to 1. Three sabotages caught: φ ends, θ ends, nulls. | FND-262 |
| `apps/nec-cli/tests/core_flags_contract.rs` | 20 | `--solver`/`--pulse-rhs`/`--exec` flag contract + usage errors; `--version`/`--help` answer on stdout with exit 0 (FND-169); `--version` and the report's last `FNEC_BUILD` line name the build's commit, HEAD's in a checkout (FND-266) | NFR-005, PH2-CHK-008, FND-169 |
| `apps/nec-cli/tests/corpus_deck_sanity.rs` | 1 | Every corpus `.nec` deck has a `GE` card | Corpus hygiene |
| `apps/nec-cli/tests/corpus_validation.rs` | 9 | Golden corpus matches references; checklist coverage (PAR002/003/005, loaded, pattern); the card-index check moved to `card_matrix.rs` (FND-235) | NFR-004, COMP-002/008, PH2-CHK-005/007 |
| `apps/nec-cli/tests/converged_reference.rs` | 8 | FND-232, FND-255, FND-256, FND-265, FND-267 — the corpus's nec2c references are nec2c on a 9× or 27× mesh: exactly the 11 expected cases carry a series, each settles within its case's gate (the drift left past its last point ≤ ⅓ of the External tolerance, on R and X), on one recorded kernel (an extended-kernel series agrees with the thin kernel at 9× within a tenth), with the reference its last point; the settle rule refuses the FND-252 5:1-step series and split-V's old 9× series; nec2c's series at the H (diverging), the tee and the T through a bar (log drift) are pinned as data, against a settling Y; fnec refined 3× moves toward it at every feedpoint and frequency, and `scripts/nec2c-fine-mesh.py` maps segment 26 → 77 of 153 and refuses an unmapped card or a sub-limit mesh.; the receive series in `corpus/series/`: nec2c's T-node current drifts (log drift), and nec2c breaks at an inverted-L's corner below about 9 radii (the 1 mm 27× step over four times the 9× step; a 0.2 mm control breaks at 81×), while the T's 0.2 mm control keeps shrinking | FND-232 |
| `apps/nec-cli/tests/current_source_ground_anchor.rs` | 2 | FND-118 — the current drive, anchored on a solver that is neither drive. | FND-118, FND-156, FND-157 |
| `apps/nec-cli/tests/current_source_junction.rs` | 2 | CLI junctioned current source: split-dipole EX-4 feedpoint Z=V/i0 matches voltage-source Z (~2e-4) | PH9-CHK-002 |
| `apps/nec-cli/tests/deck_reference_resolver.rs` | 2 | A deck reference `(tag, segment)` becomes a segment in one place: `nec_solver::find_deck_segment`; the scan refuses an inline `tag_index ==` lookup in any product source (self-checked on a planted copy). | FND-135, FND-227 |
| `apps/nec-cli/tests/deck_validator.rs` | 5 | Deck validator **refuses** a missing `EX` (error-level, FND-145) on every advertised `--solver` mode and output format; silent on well-formed decks | FR-009, EP-4 |
| `apps/nec-cli/tests/end_segment_source.rs` | 9 | FND-227 / FND-230 — a source, load or port on the segment at a free wire end: solved on every Hallén route and exec, within 2.5 % / 2 % of the wire meshed 3× finer, an end-segment load seen (ΔZ within 15 % of nec2c's), segment 0 still unnamed, a fat end judged by its deck length, the MPIE's refusal kept. | FND-227, FND-230 |
| `apps/nec-cli/tests/gpu_rerun_policy.rs` | 1 | FND-215 — no workspace member's `tests/` calls the adapter probe itself (only `gpu_concurrent_init.rs`, whose subject it is): GPU tests skip and re-run through `nec_accel::gpu_test`; the scan reads the members from the root manifest and refuses an empty or mis-rooted file set | FND-215 |
| `apps/nec-cli/tests/refusal_remedy.rs` | 6 | FND-271 — a Hallén-only refusal names the MPIE the way each frontend reaches it | — |
| `apps/nec-cli/tests/sinusoidal_chain_order.rs` | 6 | FND-233 — `--solver sinusoidal` solves a collinear chain in any `GW` order: a loaded, unequally segmented split dipole gives the same Z and the same current on every deck segment in three orders, on the plain route, within 2 % of Hallén; a scrambled T is still refused; FND-249 — a reversed middle wire fed on itself equals the forward chain in every order, and the `GX` dipole equals the one-wire dipole current for current, each segment in its own sense. | FND-233, FND-249 |
| `apps/nec-cli/tests/ex_cards.rs` | 11 | `EX` types 0/1/3 feedpoint parity; unsupported types rejected | CP-003, PH8-CHK-001/002 (baseline) |
| `apps/nec-cli/tests/exec_auto.rs` | 12 | Without `--exec` the deck picks CPU or GPU: 599 segments is the CPU to the byte, 601 (one point) the GPU within 2 Ω where there is one, a sweep below 800 stays on the CPU and past it counts its device points, a bent or loaded large deck stays on the CPU; the small-deck GPU warning prints once per run | FND-185, FND-190 |
| `apps/nec-cli/tests/exec_modes.rs` | 27 | `--exec` selection, drop-in alias resolution (a kernel name with a deck is plain fnec: the retired filename steering prints nothing, PAR-011), sandbox paths; a parallel CPU sweep is ordered and byte-identical to a single-thread run (the hybrid lane's re-run is `nec_accel::gpu_test`'s, tested there, FND-215) | DEC-003, CP-012 |
| `apps/nec-cli/tests/experimental_solver_gate.rs` | 3 | pulse/continuity refused without `--experimental-solver`; with it every text report and JSON record carries the caveat; validated solvers unchanged (FND-080) | NFR-004 |
| `apps/nec-cli/tests/gain_load_loss.rs` | 3 | FND-200: a lossy load costs gain in free space and over PEC — vs nec2c at two meshes with unloaded controls, a `--loads-config` twin equal to its `LD` card, and the RP A-digit average power gain equal to the efficiency. | FND-200 |
| `apps/nec-cli/tests/geometry_diagnostics.rs` | 18 | Fail-fast on crossing wires / tiny source; valid junctions accepted | FR-009, PH2-CHK-006 |
| `apps/nec-cli/tests/gpu_benchmark_gate.rs` | 1 | Gate G5: the GPU RP far-field kernel ≤1.5× the CPU far-field on the 2701-point grid, in-process with the device initialised once (best-of-N); skips only without a hardware adapter (FND-165) | PH5-CHK-005, PH7-CHK-002 |
| `apps/nec-cli/tests/gpu_resident_solve_cli.rs` | 3 | `--exec gpu` feedpoint Z within 2 Ω of CPU on corpus; a 2049-segment deck, past the old dispatch and `MAX_S` ceilings, solves on the device (FND-188/189) | PH7-CHK-003 |
| `apps/nec-cli/tests/gpu_rp_exec.rs` | 4 | Gate G4: `--exec gpu` RP far-field matches CPU | PH5-CHK-004 |
| `apps/nec-cli/tests/ground_diagnostics.rs` | 12 | `GN` handling: GE 1 + GN 1 over PEC, GN0/GN2 active; the finite-ground contact refusal names two models, each run at two meshes against nec2c's converged value (FND-240) | PRT-001, PH2-CHK-001/002 |
| `apps/nec-cli/tests/ground_flags_nec2c.rs` | 5 | FND-241/242/243 — `GE`/`GN` as nec2c reads them: the ground from the last `GN` alone (a raised wire's answer independent of `GE`), undefined values and `GE 1` without `GN` refused by name, a wire on the ground without `GE 1` refused with its remedy converging to nec2c at 21 and 63 segments, and nec2c's 10⁻³-of-a-segment contact band | FND-241, FND-242, FND-243 |
| `apps/nec-cli/tests/ga_gh_card_nec2c.rs` | 5 | PH10-CHK-002 — `GA` and `GH` on the MPIE: a half circle, a full circle and a helix each equal their `GW` polygon and converge to nec2c (90 / 180 / 120 segments); a quarter arc + `GM` + `GR` is the full circle, `GS` scales an arc; the Hallén refusal and nec2c's hang/garbage inputs refused by name beside the decks that solve | PH10-CHK-002 |
| `apps/nec-cli/tests/gc_card_nec2c.rs` | 4 | PH10-CHK-002 — the `GC` tapered wire as nec2c's `wire` builds it: a radius taper, a length taper and both each equal their written-out `GW` twin (Hallén and MPIE) and approach nec2c's 189-segment value step by step; `GS` scales a taper; a radius taper warns on Hallén (FND-252) and not on the MPIE; the pairing, non-positive ratio/radius, a one-segment taper and a taper under 2 radii refused by name beside the spellings that solve | PH10-CHK-002 |
| `apps/nec-cli/tests/gr_card_nec2c.rs` | 4 | FND-247 — the `GR` card as NEC-2 reads it (`I2` occurrences in all, 360°/`I2` apart): four radials from one, tags up by `I1` per occurrence (asymmetric with a parasite so the tag order shows), the corpus deck equal to the explicit pair; what nec2c stops on and fnec's old reading refused by name, with the NEC-2 spelling | FND-247 |
| `apps/nec-cli/tests/gs_card_nec2c.rs` | 5 | PH10-CHK-002 — the `GS` card as nec2c reads it: a deck in feet equals the metres deck, a wire after the card is not scaled, two cards compound and `I1`/`I2` are ignored, `GM` copies before it are scaled; each against its hand-scaled equivalent, with refining approaching nec2c's converged value; a factor that is not positive and a geometry card after `GE` refused (FND-245) | PH10-CHK-002, FND-245 |
| `apps/nec-cli/tests/gx_card_nec2c.rs` | 5 | PH10-CHK-002 — the `GX` card as nec2c reads it: half a dipole reflected in z is the dipole, two planes copy and tag as nec2c does, tag 0 stays 0, `GX` composes with `GS` and later wires, each against its written-out equivalent; a segment in or across its plane, bad fields, `GX` after `GE` and a buried z-copy refused | PH10-CHK-002, FND-245 |
| `apps/nec-cli/tests/hallen_fr_cpu_reference.rs` | 6 | Hallén FR CPU reference kernel (wgpu RP parity baseline) | PH5-CHK-003, PH7-CHK-001 |
| `apps/nec-cli/tests/json_output_contract.rs` | 5 | JSON output valid/stable, required fields, sweep records | FR-008, PH4-CHK-003 |
| `apps/nec-cli/tests/junction_band_nec2c.rs` | 4 | FND-246 — wire ends join where nec2c joins them: within 10⁻³ of the shorter segment's length by the L1 distance (a T's stem 50 µm short of the bar, a dipole in two halves axial / sideways, L1 against straight-line distance), each against nec2c 1.3.1 and fnec's own exact deck, both solvers; ends nec2c joins from one side only, and a chain of near-misses, refused | FND-246 |
| `apps/nec-cli/tests/junction_feedpoint.rs` | 11 | Junction-fed feedpoint behavior across the PH9-CHK-002 / PH9-CHK-005 boundary: a T or loop the section graph takes solves without the guard; one it refuses (a one-segment run) is refused on Hallén, naming the run and the MPIE, which then solves it (FND-251). | PH9-CHK-002, PH9-CHK-005 |
| `apps/nec-cli/tests/laplace_load.rs` | 4 | End-to-end tests for the fnec-specific Laplace-domain load (`--loads-config`, BL-IMPR-016).; a Laplace load equals its LD twin on Hallén and on the MPIE; a Laplace load on a receiving junction deck equals its LD twin, and a Laplace-loaded current source on it prices as the voltage source (FND-197/198) | — |
| `apps/nec-cli/tests/ld_loads.rs` | 5 | `LD` types 1/2/4 change impedance; unsupported warn+continue | PRT-002, PH2-CHK-003 |
| `apps/nec-cli/tests/ld_loads_per_basis.rs` | 2 | `LD` on sinusoidal/pulse/continuity: feed load shifts Z by exactly Z_L on every basis and pulse-RHS mode; off-feed sinusoidal load vs nec2c (FND-124) | PRT-002 |
| `apps/nec-cli/tests/loaded_case_tracking.rs` | 2 | Loaded non-collinear topology solves; `--allow-noncollinear` no-op | DEC-010 |
| `apps/nec-cli/tests/loads_config_validation.rs` | 3 | FND-204/207: a `--loads-config` load naming no segment, and a malformed loads file (unknown key or table, missing or non-integer index, no loads), are refused by name; explicit 0 = all and a single-segment load still apply. | FND-204, FND-207 |
| `apps/nec-cli/tests/mpie_multi_source.rs` | 1 | FND-202: two fed dipoles on `--solver mpie` match nec2c on both ports, in phase and antiphase, at two meshes. | FND-202 |
| `apps/nec-cli/tests/mpie_ground_image.rs` | 3 | FND-238 / FND-239 — `--solver mpie` near a ground is within 3 % / 1.2 % of nec2c's converged value at 1× / 3× and the finer is nearer: over a perfect ground (an L bent 0.05 m and 0.3 m up, a vertical dipole 0.05 m up) and over `GN 2` (an L 0.05 m up, four radials 0.1 m up, a vertical dipole 0.05 m up, none with a caveat); a σ = 10⁷ ground reads as `GN 1`, and sea water as the removed potential-form path (nec2c is no reference there) | FND-238, FND-239 |
| `apps/nec-cli/tests/mpie_mixed_radii.rs` | 4 | PH10-CHK-003, FND-252 — the MPIE takes each segment's own radius: a Yagi's 10 mm reflector and 2 mm driven element converge to nec2c (its extended kernel at the finest mesh); a radius step's reactance matches the first-order closed form on both solvers; a stepped Yagi element converges within 2 Ω R / 4 Ω X of its Leeson equivalent under nec2c on both, Hallén within 1 Ω of the MPIE; Hallén warns on a step beside a one-segment run, naming it, and not on a step the section graph takes, separate wires or a uniform wire | PH10-CHK-003 |
| `apps/nec-cli/tests/mpie_solver_cli.rs` | 15 | PH9-CHK-007 MPIE Phase E — `--solver mpie` CLI wiring; FND-229: an apex-fed 90° inverted V converges to nec2c's converged value; mixed radii warn on Hallén, not on the MPIE. | PH9-CHK-007 |
| `apps/nec-cli/tests/near_field_ground.rs` | 3 | FND-201: near fields over ground — over PEC a monopole and a horizontal dipole equal their free-space doubles (E and H), a vertical dipole matches nec2c; a finite ground carries a caveat. | FND-201 |
| `apps/nec-cli/tests/near_field_spherical.rs` | 2 | PH9-CHK-004: spherical NE/NH near-field grids (NEC-2 I1=1). | PH9-CHK-004 |
| `apps/nec-cli/tests/negative_load_caveat.rs` | 1 | FND-209: a negative load is named as the cause of a negative feedpoint resistance whether it is an `LD` card or a `--loads-config` load, and the caveat does not send the user to an MPIE that refuses the run. | FND-209 |
| `apps/nec-cli/tests/non_finite_currents.rs` | 4 | FND-126 / FND-127 — a solve that did not converge must not be reported as an answer, on any drive. | FND-126, FND-127 |
| `apps/nec-cli/tests/normalized_pattern.rs` | 3 | PH9-CHK-004: RP XNDA-driven normalized gain output (NORMALIZED_PATTERN). | PH9-CHK-004 |
| `apps/nec-cli/tests/par011_card_echo.rs` | 1 | PAR-011: for each of the 15 fixtures 4nec2 wrote, the parser's program-card records equal the `DATA CARD` echo its own kernel printed, card for card (XQ and the XNDA and F5/F6 fields included); the echo's two `EN` rules (the end of the file read as `EN`; an `EN` echoed with the previous card's fields) and the failure prefix of `err_seg` are pinned; `sp_patch` is refused. | PAR-011 |
| `apps/nec-cli/tests/par011_kernel.rs` | 6 | PAR-011 P2b: fnec as 4nec2's kernel, run in a 4nec2-like tree (`exe/`, `out/`) with `nec2d.tmp`'s two CRLF backslash paths on stdin, by `fnec nec2d-kernel` and under the kernel's own name with no arguments. The `.out` is the `--output-format nec2d` report in CRLF. A refused deck and a missing input are the kernel's failure (the reason, no `RUN TIME`, exit 0), and no output path exits 1; an output path with no input still gets a fresh failure file, so a stale `.out` is never read as the result. Five sabotages caught: backslashes kept, LF, a failure with `RUN TIME`, the name not dispatching, a refusal exiting 1. | PAR-011 |
| `apps/nec-cli/tests/par011_nec2d_layout.rs` | 3 | PAR-011: `--output-format nec2d` against the 15 fixtures 4nec2 and its kernel wrote. For each of the 9 decks it prints: the same line count, every line's text with numbers blanked byte-identical, each number ending in the kernel's column with its format's shape (decimals, exponent width), and every integer (segment and tag numbers, connection data, card-echo fields) the kernel's. The other 6 are refused before solving with a non-zero exit and no output (the set is pinned). The numbers are fnec's own text report's (feedpoint Z, every current, every gain) to print precision. A tokenizer self-test covers fields that run together and overflow. Six sabotages caught: a FORMAT width, `RUN TIME` dropped, a conjugated current, flipped connection signs, tag-local feed numbering, swapped gains. | PAR-011 |
| `apps/nec-cli/tests/parity_sweep.rs` | 2 | The parity sweep (stages 1–3, plus the near-ground gate): <!-- COUNT:PARITY-CELLS=627 --> **627** cells (plus the ignored `capture_nec2c_goldens`, which refreshes `parity_nec2c.txt` on a host with nec2c) over solver × exec × ground × load source × drive × geometry (incl. tilted and contact decks), each comparing two runs that must agree — `S-load`, `S-exec`, `S-fr`, `S-drive`, `S-worker`, `S-load-invalid`, `S-point`, `R-image`, `R-loss`, `R-network`, `R-remedy`, `R-reference` (nec2c), `R-solver`, `R-converge`, `R-lowground` (nec2c GN 2 at 0.025–0.1 λ, two meshes) — with outcomes, counts and reached decisions pinned in `parity_manifest.txt`. | FND-197..216 |
| `apps/nec-cli/tests/parser_warnings.rs` | 23 | A card that cannot change the antenna warns and solves, one that would (`GX`) or an unknown one is refused by name (FND-236); `TL` segments; well-formed NT solved, malformed NT refused | COMP-001, PRT-002 |
| `apps/nec-cli/tests/project_cmd.rs` | 5 | GAP-015's acceptance criterion names "explicit CLI/API entry points" for Markdown project import and export. | FND-006, FND-016, GAP-015 |
| `apps/nec-cli/tests/pt_print_control.rs` | 4 | PH9-CHK-004: PT (print-control) card runtime semantics — filter the segment current output by mode / tag / segment range. | PH9-CHK-004 |
| `apps/nec-cli/tests/receive_junction.rs` | 2 | CLI junctioned receive: split-dipole receive sweep has dipole shape and matches transmit by reciprocity (0.025 dB) | PH9-CHK-002 |
| `apps/nec-cli/tests/receive_pattern.rs` | 2 | PH9-CHK-001: incident-plane-wave receive-pattern sweep. | PH9-CHK-001 |
| `apps/nec-cli/tests/report_contract.rs` | 7 | Report v1 headers/rows; RP/sweep/load tables; section ordering | FR-005, PH2-CHK-004 |
| `apps/nec-cli/tests/resonance_contract.rs` | 5 | `--resonance` convergence, unbounded fail, missing-flag usage; refuses what every frontend refuses — unsupported load, negative frequency, crossing wires (FND-173) | FR-010, PH3-CHK-008 |
| `apps/nec-cli/tests/rp_avg_power_gain.rs` | 2 | PH9-CHK-004: RP XNDA `A` digit — average power gain. | PH9-CHK-004 |
| `apps/nec-cli/tests/scriptability_contract.rs` | 25 | Scripting/drop-in alias contract (streams under kernel names; with no arguments a kernel name is 4nec2's kernel and asks stdin for two files, PAR-011); temp-file & path handling | NFR-005, GAP-011, PH2-CHK-008 |
| `apps/nec-cli/tests/sinusoidal_a2_regression.rs` | 2 | Sinusoidal solver tracks Hallén on dipole + sweep | DEC-011, PH6-CHK-003 |
| `apps/nec-cli/tests/sinusoidal_routing.rs` | 2 | FND-203: `--solver sinusoidal` refuses bent, split, junction and loop decks by name (path route and graph-declined topology both), and still solves straight and collinear decks on its basis. | FND-203 |
| `apps/nec-cli/tests/sommerfeld_ground_cli.rs` | 5 | PH9-CHK-006: `fnec --ground-solver sommerfeld` must correct the near-ground feedpoint impedance of a low horizontal dipole to the surface-wave-inclusive (nec2c GN2) value, flipp…; a plane-wave receive deck says the correction cannot apply (FND-170) | PH9-CHK-006 |
| `apps/nec-cli/tests/sommerfeld_ground_cli.rs` | 7 | PH9-CHK-006: `fnec --ground-solver sommerfeld` must correct the near-ground feedpoint impedance of a low horizontal dipole to the surface-wave-inclusive (nec2c GN2) value, flipp…; a plane-wave receive deck says the correction cannot apply (FND-170) | PH9-CHK-006 |
| `apps/nec-cli/tests/sweep_config_replaces_fr.rs` | 2 | FND-210: under `--sweep-config` the deck's FR cards neither refuse the run (a negative FR the sweep replaces) nor describe it (the superseded-FR caveat); without it both still apply. | FND-210 |
| `apps/nec-cli/tests/sweep_memory_budget.rs` | 3 | FND-187: a small `FNEC_SWEEP_MEMORY_BUDGET_MB` caps the points in flight and says so, with every point still out in frequency order; an ample budget is silent; a malformed one is reported | FND-187 |
| `apps/nec-cli/tests/sweep_contract.rs` | 8 | Sweep point/list/linear produce correct frequency blocks; `--sweep-config` **supplies** frequencies for an FR-less deck, and a deck with no frequency from any source is refused (FND-070) | FR-007, PH3-CHK-006 |
| `apps/nec-cli/tests/sweep_partial_output.rs` | 2 | A sweep prints the points it computed, even when one of them fails. | FND-033 |
| `apps/nec-cli/tests/taper_cli.rs` | 2 | `fnec taper` — the Leeson step-tapered-radius correction (BL-IMPR-014). | — |
| `apps/nec-cli/tests/template_contract.rs` | 5 | TOML/JSON var substitution; undefined-token error | PH3-CHK-007 |
| `apps/nec-cli/tests/tl_cards.rs` | 5 | NEC-2 `TL` card matches nec2c; retired fnec layout refused with the NEC-2 rewrite; integer-valued NEC-2 card not mistaken for the old layout; zero length = segment-centre distance; non-Hallén solver refused | PRT-002, PH2-CHK-003 |
| `apps/nec-cli/tests/topology_caveat_reach.rs` | 2 | FND-272 — the topology caveats are reached on the experimental bases, and not by a receiving junction on PEC ground | — |
| `apps/nec-cli/tests/topology_fallback.rs` | 12 | Non-single-chain fallback across solver/pulse/exec/sinusoidal/loaded | DEC-010/011 |
| `apps/nec-cli/tests/traceability.rs` | 1 | G2 — machine-enforced requirements traceability (review-260821). | — |
| `apps/nec-cli/tests/worker_deadline.rs` | 2 | FND-101 — a worker that accepts a task and never answers must not wedge the run. | FND-101 |
| `apps/nec-cli/tests/worker_gpu_exec.rs` | 1 | Distributed GPU dispatch through worker pool (mixed gpu/cpu) | PH7-CHK-004 |
| `apps/nec-cli/tests/worker_infinite_vswr.rs` | 2 | FND-117 — one unusable result must not destroy the worker pool. | FND-117 |
| `apps/nec-cli/tests/worker_integration.rs` | 10 | Hosts config, capability cache, subprocess round-trip | PH6-CHK-006/007 |
| `apps/nec-cli/tests/worker_mesh.rs` | 2 | FND-227 — a worker on another mesh is refused, not blended into a sweep: a stand-in for a pre-refinement fnec (no `mesh` in its result) fails its task with the reason and stays in the pool; this build's own worker answers. | FND-227 |
| `apps/nec-cli/tests/worker_poison_budget.rs` | 3 | FND-102 — one task that kills workers must not kill the pool; FND-244 — a task whose write stalls on workers that stopped reading is not blamed for them, and reaches a live one. | FND-102, FND-244 |
| `apps/nec-cli/tests/worker_task_fault.rs` | 1 | FND-117 — a task fault must not evict the worker that reported it. | FND-117 |
| `apps/nec-gui/tests/gui_smoke.rs` | 135 | Headless GUI state machine + solve pipeline; run-identity guards; editor save binding (FND-103) | PRT-004, PH3-CHK-009/010/011 |
| `crates/nec_accel/tests/gpu_hallen_solve.rs` | 1 | Gate G7: GPU Z-fill + CPU Hallén solve end-to-end | PH5-CHK-007 |
| `crates/nec_accel/tests/gpu_microbench.rs` | 1 | Microbench separates per-dispatch time from device init | PH7-CHK-002 |
| `crates/nec_accel/tests/gpu_backoff.rs` | 1 | FND-234 — a cached device failure is returned without a rebuild until its backoff passes (also past the 60 s cap), a device lost under a dispatch is one more counted failure, and past the backoff the next caller asks the driver again: a fresh device on a GPU host, a counted fresh failure without one; its own binary, because the cache is process-global | FND-234 |
| `crates/nec_accel/tests/gpu_ceilings.rs` | 5 | The GPU size ceilings (FND-188/189): the 2-D dispatch grid at the 65 535 boundary, the storage-binding capacity, every indexed shader entry reads the grid (structural), a 2049-segment fill on the device, a decline one past the device's capacity names its reason; device tests skip without a hardware adapter | FND-188, FND-189 |
| `crates/nec_accel/tests/gpu_concurrent_init.rs` | 1 | FND-186: four threads' first wgpu use must not segfault; alone in its binary, because the crash needs an uninitialised driver (10 of 10 with the lock removed) | FND-186 |
| `crates/nec_accel/tests/gpu_rerun_fresh_context.rs` | 1 | FND-215 — with a failed device request pinned in the shared context, a re-run builds a fresh device and the fill succeeds on the second attempt; its own binary, because the cache is process-global | FND-215 |
| `crates/nec_accel/tests/shader_structure.rs` | 2 | FND-193: no shader declares a `const` array (naga gives its function-local copy an `ArrayStride` that Vulkan validation rejects); and none uses workgroup memory or a barrier (the LU solve's reductions go through per-lane partials and a dispatch boundary); scans the shader directory, so a new shader is covered | FND-193 |
| `crates/nec_accel/tests/gpu_context_once.rs` | 1 | FND-186: eight concurrent first callers build one shared device (own binary: the build counter is process-global) | FND-186 |
| `crates/nec_accel/tests/gpu_pivot.rs` | 2 | FND-254 — the GPU solve's pivot search on chosen matrices (column maxima off the diagonal, zeros in the diagonal lane's rows), through the production resident solve, against f64 partial-pivoting elimination: fewer rows than lanes, and more | FND-254 |
| `crates/nec_accel/tests/gpu_resident_solve.rs` | 5 | Fully GPU-resident Hallén fill+solve parity, centred and asymmetric feeds (sin homogeneous column, FND-158); a hardware adapter makes `None` a failure (FND-163); a 301-segment dipole solves on the device; the solve shader uses no storage barrier (FND-185); no single-invocation entry point reads the LU factor (the triangular solves run one dispatch per column) | PH7-CHK-003 |
| `crates/nec_accel/tests/gpu_zmatrix_parity.rs` | 1 | Gate G6: GPU Z-fill element-wise parity vs CPU | PH5-CHK-006 |
| `crates/nec_parser/tests/card_matrix.rs` | 2 | `docs/card-support-matrix.md`, the one card table, lists exactly `nec_parser::OTHER_CARDS` with the same treatment (FND-236), and has a row for every card the parser reads (PH3-CHK-001, FND-235) | FND-235, FND-236, PH3-CHK-001 |
| `crates/nec_project/tests/project_roundtrip.rs` | 20 | `ProjectFile` TOML/Markdown round-trip + errors | FR-004, PH3-CHK-004/005, GAP-015 |
| `crates/nec_solver/tests/asymmetric_current_nec2c.rs` | 6 | Asymmetric currents vs nec2c: off-centre feed (plain, sinusoidal, conductor path), vertical dipole over PEC, offset parasitic, current source = voltage drive (FND-158) | NFR-004 |
| `crates/nec_solver/tests/bend_corner_nec2c.rs` | 3 | Hallén on bent conductors vs nec2c at two meshes each — a 90° L, a two-bend U (the interior-section branch), an off-apex inverted-V; the error must shrink with refinement (FND-162 stage 1b) | FND-162 |
| `crates/nec_solver/tests/bend_receive_nec2c.rs` | 5 | Plane-wave receive on bent conductors vs nec2c at two meshes each — a 90° L, the same L with one arm unlit (fed only through the bend), a two-bend U; error must shrink; planned == one-shot receive solve; the one-segment-run fallback predicate (FND-162) | FND-162 |
| `crates/nec_solver/tests/collinear_merge.rs` | 7 | PH9-CHK-002 (collinear case): a straight conductor split across several GW cards must solve as one wire. | PH9-CHK-002 |
| `crates/nec_solver/tests/conductor_path_corpus.rs` | 3 | FND-132 — the corpus must reach the conductor-path basis. | FND-121, FND-132, PH9-CHK-002 |
| `crates/nec_solver/tests/current_source.rs` | 4 | PH8-CHK-001: validate the current-source (EX type 4) Hallén solve. | PH8-CHK-001 |
| `crates/nec_solver/tests/current_source_junction.rs` | 3 | Current-source (EX type 4) degree-2 junction solve: split-dipole + inverted-V Z=V/i0 == voltage-source Z (~2–3e-4); i0 linearity | PH9-CHK-002 |
| `crates/nec_solver/tests/end_condition_nec2c.rs` | 5 | Hallén free-end condition vs captured nec2c: dipole, reactance gap shrinks with N, coupled pair at 1 m, 5-element Yagi (FND-156) | NFR-004 |
| `crates/nec_solver/tests/farfield_e_field_nec2c.rs` | 1 | PAR-011: the far field's complex components `R·E_θ`, `R·E_φ` (NEC-2's scaling and phase) against nec2c converged at 9× — free space, perfect ground, `GN 0`, and crossed dipoles in quadrature (elliptical) — each nec2c series asserted settled (r < 0.7, tail ≤ tol/3), fnec within 4.0 % at 1× and 1.75 % at 3×, shrinking. | PAR-011 |
| `crates/nec_solver/tests/finite_ground_rp.rs` | 5 | PH8-CHK-006: radiation pattern over finite ground via the Fresnel reflection-coefficient far field. | PH8-CHK-006 |
| `crates/nec_solver/tests/general_junction.rs` | 10 | PH9-CHK-002 (general junction case): a single physical conductor whose two arms meet at a degree-2 junction — start-to-start splits and bent inverted-V feeds — must solve to a p… | PH9-CHK-002 |
| `crates/nec_solver/tests/gm_nec2c.rs` | 8 | FND-119 — the `GM` card, pinned against `nec2c`. | FND-119 |
| `crates/nec_solver/tests/ground_contact.rs` | 10 | Wires on PEC ground by explicit images: monopole and grounded array vs nec2c, identity with the doubled free-space deck, base-load identity, unrepresentable contacts and unmirrored drives refused (FND-082); a current source on contact prices as the voltage source to round-off (straight, bent, top-hat); TL/NT on contact equal an independently drawn free-space double and track nec2c at two meshes; a current source with a network stays refused | NFR-004 |
| `crates/nec_solver/tests/ground_impedance.rs` | 3 | Near-ground impedance: ground ΔZ vs nec2c — horizontal (R drops), vertical near-ground (R rises +18Ω), and 0.25λ vs Sommerfeld truth | PH9-CHK-006 |
| `crates/nec_solver/tests/lossy_tl.rs` | 3 | PH8-CHK-005: lossy transmission line — fnec's F8 extension (matched-line loss in dB) on the NEC-2 TL layout (FND-111). | FND-111, FND-123, PH8-CHK-005 |
| `crates/nec_solver/tests/mpie_farfield.rs` | 3 | PH9-CHK-007 MPIE Phase C — far-field from the recovered MPIE currents. | PH9-CHK-007 |
| `crates/nec_solver/tests/hallen_radius_step.rs` | 2 | FND-252 — Hallén takes a radius step on the section graph on every drive: `EX 4` prices as `EX 0` on a stepped wire to 1e-9 (the current-source entry hands it to the routed solve), and a load on the fed segment adds exactly its impedance | FND-252 |
| `crates/nec_solver/tests/mpie_free_space.rs` | 4 | PH9-MPIE Phase A — free-space MPIE straight-wire core, external gates. | — |
| `crates/nec_solver/tests/mpie_loads.rs` | 7 | PH10-CHK-004 — the MPIE stamps `LD` loads through the shared session: a load on the fed segment adds exactly its impedance (free space, `GN 1`, `GN 2`, an apex-fed inverted V); an off-centre series load, a trap dipole, a copper dipole, a Y with a loaded arm and a loaded dipole 0.05 λ over `GN 2` converge to nec2c; a load naming no segment, from a card or `--loads-config`, refused by the session itself | PH10-CHK-004 |
| `crates/nec_solver/tests/mpie_ground.rs` | 8 | PH9-CHK-007 MPIE Phase D — Sommerfeld ground IN the Z-matrix. The perfect-ground image keeps each segment's radius (PH10-CHK-003). | PH9-CHK-007 |
| `crates/nec_solver/tests/mpie_junction.rs` | 4 | PH9-CHK-007 MPIE Phase B — degree-N junctions, external gates. | PH9-CHK-007 |
| `crates/nec_solver/tests/mpie_loop.rs` | 2 | PH9-CHK-007 MPIE Phase B (B3) — closed loops. | PH9-CHK-007 |
| `crates/nec_solver/tests/mpie_nec2c.rs` | 8 | MPIE vs nec2c through the session: dipole, 5-element Yagi, Y-junction (FND-157) | NFR-004 |
| `crates/nec_solver/tests/near_field.rs` | 3 | PH9-CHK-004: near electric-field computation (NE card), validated against the far field it must reduce to at large range and by dipole symmetry. | PH9-CHK-004 |
| `crates/nec_solver/tests/network_solve.rs` | 7 | TL/NT solved as networks across the port gaps: one-port NT ≡ LD (straight and conductor-path), shunt across the feed analytic, same-segment one-port, pair+TL vs nec2c, feed load with a network present, other drives refused (FND-123) | NFR-004 |
| `crates/nec_solver/tests/nt_network.rs` | 2 | PH8-CHK-004: `NT` networks, read as the admittance parameters they are (FND-123). | FND-123, PH8-CHK-004 |
| `crates/nec_solver/tests/planewave_junction.rs` | 2 | Receive-side degree-2 junction solve through the production seams: split-dipole receive == per-wire solver (~1e-11); bent inverted-V reciprocity — angle spread AND the absolute level `R·λ²/(π·η₀·|Z|²)` from the transmit solve (0.09 %; the spread alone could not see a bend error) | PH9-CHK-002, FND-162 |
| `crates/nec_solver/tests/planewave_nec2c.rs` | 10 | PH8-CHK-002: validate the incident-plane-wave Hallén solve. | PH8-CHK-002 |
| `crates/nec_solver/tests/power_budget_nec2c.rs` | 1 | PAR-011: NEC-2's structure loss (½·Σ\|Iₙ\|²·Re Zₙ) and efficiency (input less losses) against nec2c converged at 9×, on a series RLC with copper, copper alone and a parallel RLC; each nec2c series asserted settled within a third of the bound; fnec within 0.003 / 0.002 in loss fraction and 0.3 / 0.25 points of efficiency at 1× / 3×, not moving away. | PAR-011 |
| `crates/nec_solver/tests/pulse_rhs_scaling.rs` | 1 | Pulse RHS inverse-wavelength scaling | PRT-002 |
| `crates/nec_solver/tests/regularisation_bias.rs` | 1 | FND-164 — the Hallén solve's Tikhonov term must not bias the answer. | FND-164 |
| `crates/nec_solver/tests/sommerfeld_ground.rs` | 2 | PH9-CHK-006: the Sommerfeld reflected-field kernel must reproduce nec2c's exact GN2 near-ground impedance for a horizontal dipole — in particular the surface-wave SIGN FLIP belo… | PH9-CHK-006 |
| `crates/nec_solver/tests/straight_rule.rs` | 2 | One rule for "straight" (FND-172/175): a wire split into two cards with a rounded-coordinate kink equals the one-card wire and tracks nec2c; a stepped-radius element tracks the MPIE at two meshes, converging (at a radius step nec2c is no reference, FND-252) | FND-172, FND-175 |
| `crates/nec_solver/tests/transverse_nec2c.rs` | 3 | The transverse-divergence term from sources sharing no node (FND-162 stage 4, FND-171, FND-174) vs nec2c at two meshes, converging: a 45° dipole over PEC, an inverted-V over PEC (== its explicit image in free space), a wire over a vertical dipole (antisymmetric at 0.7 m and 5 cm) | FND-162, FND-171, FND-174 |
| `crates/nec_solver/tests/graph_nec2c.rs` | 21 | Hallén on the section graph (FND-162 stages 2+3) vs nec2c at two meshes, converging: a stem-fed Y and T, a T fed on its node, a dipole with a centre stub (a regression gate — the fallback passes it too), a 1 λ square loop, a loop with a stub, the loop over PEC; and Kirchhoff at the Y's node read off the solved currents (1e-9); stage 5: a loaded Y, a T with a coil on its node segment, a loop loaded beside a corner, and the exact feed-load identity; on PEC ground: a top-hat monopole equals its free-space H, the H's Hallén and MPIE solved live at two meshes of one antenna and converging together (FND-264), its same-mesh gap to nec2c pinned as a tripwire on a checked-in deck, a folded monopole against nec2c, and neither flagged nor warned; a current source on a Y, a loop and a loaded T equals the voltage gap; a Y with a TL between its arms and two loops on a phasing line against nec2c | FND-162 |
| `crates/nec_solver/tests/graph_receive_nec2c.rs` | 7 | FND-162 stage 5: the plane-wave receive solve on the section graph — a 1 λ loop broadside against nec2c's currents at two meshes, converging; a T at 45° and a loaded T at 21 / 63 per wire against one nec2c reference on a 27× mesh from checked-in series decks, away from the node error + 3 × nec2c's step under 1 % and shrinking, at the node a band and fnec's own convergence (FND-265); a loaded plane-wave junction deck is refused; loaded decks (FND-197/198): a loaded T and loop vs nec2c at two meshes, the compensation identity through the driven graph, a loaded current source pricing as the voltage source, no loads given for an LD deck refused (the loaded-refusal test is gone) | FND-162 |
| `crates/nec_solver/tests/planewave_ground_nec2c.rs` | 6 | FND-170: a plane wave over perfect ground against nec2c's currents at two meshes, converging — a horizontal and a vertical dipole, an inverted-V (bent-path route), a T (section graph), an elliptic wave; a wave from below the plane (single or a sweep row), wires touching the ground and finite ground are refused by name | FND-170 |
| `crates/nec_solver/tests/planewave_ground_contact.rs` | 5 | FND-170: a plane wave on wires touching PEC ground — the contact deck equals its free-space double lit from θ and from 180° − θ to round-off (straight and bent); a monopole and an inverted-L (21 / 63 per wire against nec2c's 9× from checked-in series decks, FND-265) against nec2c at two meshes, converging; the sweep route doubles the structure too; a loaded contact deck equals its loaded double on both routes (FND-197) | FND-170 |
| `crates/nec_solver/tests/planewave_finite_ground_nec2c.rs` | 4 | FND-170 PR 2: a plane wave over finite ground — the ratio of the current over `GN 2` to the same deck's free-space current against nec2c's, 3 λ up, one deck per polarization (θ̂ isolates `rrv`, φ̂ `rrh`), converging; a pinned band at 0.24 λ that measures fnec's matrix model; a ground of εr = 1, σ = 0 receives exactly as free space | FND-170 |
| `crates/nec_solver/tests/negative_sigma.rs` | 1 | FND-194: a `GN` card with a negative `SIG` (NEC's ε'' form) solves exactly as its conductivity equivalent, through the matrix (feedpoint impedance) and the far field (gain over ground), and not as the lossless ground it was clamped to | FND-194 |
| `crates/nec_solver/tests/interior_joins.rs` | 4 | FND-192: a T drawn with its stem on a bar joint is the T (equal to the T drawn as halves, against nec2c), an X crossing at a shared joint is four arms, the merged wire list sees the junction, a plane wave on it is received exactly as on the T drawn as halves | FND-192 |
| `crates/nec_worker/tests/gpu_exec.rs` | 2 | Worker-level GPU execution vs CPU parity | PH7-CHK-004 |

Integration subtotal: <!-- COUNT:INTEGRATION-SUBTOTAL=810 --> **810** test
functions across the `tests/` binaries listed above.

## Unit tests (in `src/`)

<!-- Counts below are CHECKED, not typed: `scripts/check-test-catalog-counts.py`
     re-derives every number in this section from `cargo test --workspace -- --list`
     and fails the build on drift. Re-measure with `--list-only`. -->

| Crate | # `#[test]` | Concentration |
|:------|:------------|:--------------|
| `nec_solver` | 274 | loads, geometry, excitation, linear, matrix, farfield, basis, tl, planewave, sommerfeld permittivity, finite-ground remainder (split, table, graded panels) |
| `nec_worker` | 98 | worker, solve, capability, protocol, hosts, pool, controller, ssh_worker |
| `nec-gui` | 98 | app_state, model_doc, mesh, camera, solve |
| `apps/nec-cli` | 50 | main, exec_profile, kernel (the PAR-011 kernel path, CRLF and warnings, its failure), sweep_config, warnings, solve_session (CPU points concurrently, GPU points in turn, hybrid's GPU lane and CPU pool at once — every point once — and the lane stops after a fallback) |; the sweep's memory budget (FND-187: one slot never overlaps two points, two slots do, hybrid keeps a CPU worker; the budget arithmetic)
| `nec_parser` | 40 | lib (incl. the FND-236 card treatments, FND-237 comma fields and the PAR-011 program-card records), template |
| `nec_accel` | 40 | kernel_reference 20, lib 4, `wgpu_device` RUST_LOG filter 3 (FND-190), GPU wait timeout 5 (FND-196), failure backoff 3 (FND-234), `gpu_test` re-run policy 4 (FND-215) |
| `nec_report` | 28 | lib 25, fortran 3 (NEC-2 FORMAT lines lifted from the kernel fixtures, records, edge cases) |
| `nec_project` | 21 | lib 21 |
| `nec_model` | 7 | lib 7 |

Unit subtotal: <!-- COUNT:UNIT-SUBTOTAL=656 --> **656** `#[test]` functions.

## Totals

- **Test functions**: <!-- COUNT:WORKSPACE-TOTAL=1473 --> **1473** = 656 unit + 810 integration + **7 doctests**.
- **`cargo test --workspace` aggregate**: **1098 passing, 0 failed, 2 ignored**,
  measured 2026-09-07 — the authoritative pass count in [test-results.md](test-results.md).

Doctests are counted separately on purpose. `cargo test --workspace -- --list`
prints them under `Doc-tests <crate>` headers that carry no `Running` line, so a
parser that only tracks `Running` charges all seven to whichever `tests/*.rs`
binary happened to be listed last. The first version of the checker did exactly
that, inflating one row by 7 and the integration subtotal with it, and **passed**
— it was self-consistent with its own bug. Caught in review.

**The configuration matters, so it is stated rather than implied.** These numbers are
from a `--workspace` run. Feature unification turns on `nec_accel/wgpu` there, which
adds four `nec_accel` lib tests (26 rather than 22). Until FND-144 it was also the
only reason that crate's four `tests/*.rs` binaries compiled — `cargo test -p
nec_accel` alone failed to build them; the crate's tests now enable the feature
themselves, and a per-crate run gives the same 26. A count is still not a property
of the tree by itself; it is a property of the tree and the build configuration
together.

These counts are derived, not typed: `scripts/check-test-catalog-counts.py` enumerates
what the harness will actually run and fails on drift. The previous figures — "~532
across 53 test binaries" — were hand-maintained and had drifted by more than a factor
of two while every one of them looked precise (FND-143). The per-file integration
table above is derived and checked the same way since 2026-09-27; before that it had
18 wrong rows and 36 test files with no row at all.

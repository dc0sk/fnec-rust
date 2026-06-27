---
project: fnec-rust
doc: docs/cli-guide.md
status: living
last_updated: 2026-06-23
---

# CLI Guide — fnec (v0.2.0)

`fnec` is the command-line frontend for fnec-rust.  It reads a NEC deck file,
runs the configured solver, and prints a versioned text report to stdout
(feedpoints, currents, and RP-driven radiation pattern when requested).
Diagnostics are written to stderr.

## Synopsis

```
fnec [--solver <hallen|pulse|continuity|sinusoidal>] [--pulse-rhs <raw|nec2>] [--exec <cpu|hybrid|gpu>] [--sin-fallback-rel-max <value>] [--allow-noncollinear-hallen] [--ex3-i4-mode <legacy|divide-by-i4>] [--bench] [--bench-format <human|csv|json>] [--sweep-config <file.toml>] [--vars <vars.toml|vars.json>] <deck.nec>
fnec sweep --resonance <file.nec.toml>
```

Exit codes: **0** success, **1** I/O or solver error, **2** usage error.

Compatibility profile note:

- The CLI now includes a filename-steered compatibility profile scaffold for 4nec2-style external kernel replacement workflows.
- Drop-in profile activation uses an explicit binary-stem contract: known NEC2MP kernel names (`nec2dxs500`, `nec2dxs1K5`, `nec2dxs3k0`, `nec2dxs5k0`, `nec2dxs8k0`, `nec2dxs11k`, case-insensitive) or names containing `4nec2`.
- When that profile is active, default execution is steered to `--exec hybrid` unless `--exec` is explicitly provided.
- Diagnostics explicitly distinguish the two cases: "default execution path steered" vs "preserving explicit --exec=...".
- This currently changes execution-mode defaulting only; argument/output contract compatibility work remains tracked in backlog parity item `PAR-011`.
- In the native profile (normal `fnec` binary name), when `--exec` is omitted, startup now runs a quick execution probe and auto-selects the best available execution mode for the current workload shape.

## Options

| Option | Values | Default | Description |
|--------|--------|---------|-------------|
| `--solver` | `hallen` \| `pulse` \| `continuity` \| `sinusoidal` | `hallen` | MoM solver to use (see below) |
| `--pulse-rhs` | `raw` \| `nec2` | `nec2` | RHS scaling for pulse/continuity modes |
| `--exec` | `cpu` \| `hybrid` \| `gpu` | `auto` (native profile), `hybrid` (4nec2 drop-in profile) | Execution backend preference. `hybrid` uses split-lane FR scheduling (CPU-parallel lane + GPU-candidate lane) with deterministic ordered output; GPU-candidate lane points currently fall back to CPU with explicit diagnostics until GPU kernels are wired. `gpu` currently falls back to CPU kernels with explicit diagnostics |
| `--sin-fallback-rel-max` | positive float | `1e-2` | Sinusoidal-only relative residual threshold for guarded fallback to Hallen. CLI flag takes precedence over `FNEC_SIN_FALLBACK_REL_MAX` env var |
| `--allow-noncollinear-hallen` | flag | off | Compatibility placeholder; accepted but silently ignored. Has no effect on solver behaviour (Phase 1). |
| `--ex3-i4-mode` | `legacy` \| `divide-by-i4` | `legacy` | EX type 3 runtime semantics: `legacy` keeps type 3 == type 0 behavior; `divide-by-i4` enables experimental source normalization using I4 as divisor when I4>0 |
| `--bench` | flag | off | Enable benchmark instrumentation plumbing (also used by the GPU benchmark timing gates) |
| `--bench-format` | `human` \| `csv` \| `json` | `human` | Emit machine-readable benchmark records to stderr as `bench_csv:` or `bench_json:` lines while keeping the normal human-readable report on stdout |
| `--sweep-config` | `<file.toml>` | — | Load a TOML frequency-sweep spec (range or explicit list); overrides the `FR` card frequency list for a batch solve. See `examples/sweep-spec.toml`. |
| `--vars` | `<file.toml\|file.json>` | — | Load a flat key→value map and substitute `$VAR` tokens in the deck before parsing. TOML (any extension except `.json`) and JSON flat-object files are both accepted. An undefined token causes a non-zero exit with a diagnostic. |

## Subcommands

### `fnec sweep --resonance <file.nec.toml>`

Runs a binary-search resonance-targeting pass over one template variable to
find the value at which the feedpoint reactance matches a target (typically 0 Ω
for series resonance).

The `.nec.toml` file is a TOML file containing two required tables:

```toml
[search]
var                   = "HALF_LEN"   # template variable to search
lo                    = 4.5          # lower bound
hi                    = 6.0          # upper bound
target_reactance_ohm  = 0.0          # target Z_im
tolerance_ohm         = 0.5          # convergence tolerance (default 0.5)
max_iter              = 50           # max bisection iterations (default 50)

[deck]
template = """
GW 1 51 0 0 -$HALF_LEN 0 0 $HALF_LEN 0.001
GE
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
EN
"""
```

The deck template must contain exactly one `FR` card (single frequency), and
the named variable must appear at least once as a `$VAR` token.

**Output** (stdout, structured text):

```
RESONANCE_SEARCH_RESULT
VAR HALF_LEN
CONVERGED_VALUE 5.192382
Z_RE 73.112345
Z_IM -0.312456
ITERATIONS 14
CONVERGED true
```

Exit code **0** when converged or when max iterations reached (with a warning
on stderr); **1** if the root is not bracketed or a solver error occurs; **2**
for usage errors.

See `examples/resonance-search.nec.toml` for a complete worked example.

## Solver modes

### `hallen` (recommended for collinear wire sets)

Augmented Hallén integral equation with 8-point Gauss-Legendre quadrature and
analytic singularity subtraction.  Produces physically accurate feedpoint
impedance for thin-wire antennas when all wires are collinear with the driven
segment axis. Non-collinear topologies currently return an explicit unsupported
topology error instead of a misleading impedance.

The `--allow-noncollinear-hallen` flag is accepted for compatibility but is
currently silently ignored — passing it has no effect on solver behaviour.
The hard-fail guardrail remains active in Phase 1 regardless.

Validated result — 51-segment λ/2 dipole, 14.2 MHz:

```
74.242874 + j13.899516 Ω  (Python MoM reference: 74.23 + j13.90 Ω)
```

### `pulse` (EXPERIMENTAL)

Pulse-basis Pocklington EFIE.  **Known to diverge** from the physical solution
as segment count increases — do not use for production work. Use `hallen` or
`sinusoidal` for accurate supported-path runs.

### `continuity` (EXPERIMENTAL)

Same Pocklington matrix as `pulse`, but solves via a continuity-enforcing rooftop
basis transform applied per wire chain on multi-wire decks when each wire has
at least two segments. Falls back to `pulse` when topology is infeasible for
the basis transform or when residual exceeds 1e-3. Subject to the same fundamental
divergence as `pulse`.

### `sinusoidal`

Sinusoidal-basis solve path for the Hallen thin-wire system, with guarded fallback
when the residual-quality budget is exceeded.
If the projected sinusoidal solve exceeds the residual budget on a single
collinear chain, the CLI falls back to `hallen` and reports
`SOLVER_MODE sinusoidal->hallen(residual)`.

Residual budget precedence:

- `--sin-fallback-rel-max <value>` (if provided)
- `FNEC_SIN_FALLBACK_REL_MAX` environment variable
- built-in default `1e-2`

## `--pulse-rhs` values

Applies to `pulse`, `continuity`, and `sinusoidal` modes.

| Value | Behaviour |
|-------|-----------|
| `nec2` | Scale RHS by `−1/(λ)` — NEC2 sign/wavelength convention |
| `raw` | Use the excitation vector as-is (diagnostic use only) |

## Output format

Report contract v1 is a stable, versioned text layout:

```
FNEC FEEDPOINT REPORT
FORMAT_VERSION 1
FREQ_MHZ <mhz>
SOLVER_MODE <mode>
PULSE_RHS <Raw|Nec2>

FEEDPOINTS
TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM
<tag> <seg> <v_re> <v_im> <i_re> <i_im> <z_re> <z_im>
...

SOURCES
N_SOURCES <n>
TYPE TAG SEG I4 V_RE V_IM
...

LOADS
N_LOADS <n>
TYPE TAG SEG_FIRST SEG_LAST F1 F2 F3
...

CURRENTS
TAG SEG I_RE I_IM I_MAG I_PHASE
...

RADIATION_PATTERN
N_POINTS <n>
THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO
...
```

Feedpoint table columns:

| Column | Unit | Description |
|--------|------|-------------|
| TAG | — | GW tag number |
| SEG | — | 1-based segment index within the wire |
| V_RE / V_IM | V | Source voltage real/imag (`v_ex × segment_length`) |
| I_RE / I_IM | A | Current real/imag at the driven segment |
| Z_RE / Z_IM | Ω | Feedpoint impedance real/imag (`V_source / I`) |

The impedance is computed as:
$$Z_{\mathrm{in}} = \frac{V_{\mathrm{source}}}{I_{\mathrm{source}}} = R + jX$$

Formatting and ordering rules:

- Fixed-point numeric formatting with 6 decimals
- Exactly 8 whitespace-separated numeric columns per data row
- One data row per driven segment (zero-excitation segments skipped)
- `SOURCES` appears when one or more `EX` cards are present, with source definitions in deck/card order
- `LOADS` appears when one or more `LD` cards are present, with load definitions in deck/card order
- `RADIATION_PATTERN` appears only when at least one `RP` card is present in the deck

## Diagnostics (stderr)

A diagnostic line is always printed after the solve:

```
diag: mode=hallen pulse_rhs=Nec2 exec=cpu freq_mhz=14.200000 abs_res=3.456789e-10 rel_res=2.345678e-08 diag_spread=1.000000e0 sin_rel_res=0.000000e0 sin_fallback_rel_max=1.000000e-02
```

| Field | Description |
|-------|-------------|
| `mode` | Effective solver path used (may differ from `--solver` if fallback occurred) |
| `pulse_rhs` | Active `--pulse-rhs` setting |
| `exec` | Effective execution mode (`cpu`, `hybrid`, `gpu(cpu-fallback)`) |
| `freq_mhz` | Frequency point solved for this report block |
| `abs_res` | Absolute L2 residual ‖Ax − b‖ |
| `rel_res` | Relative L2 residual ‖Ax − b‖ / ‖b‖ |
| `diag_spread` | Conditioning proxy: max/min diagonal magnitude ratio of solved matrix |
| `sin_rel_res` | Sinusoidal pre-fallback relative residual (0 for non-sinusoidal paths) |
| `sin_fallback_rel_max` | Active sinusoidal residual fallback threshold after CLI/env/default precedence |

When `--bench-format csv` is enabled, one header plus one machine-readable line per solved frequency point is emitted to stderr:

```
bench_csv:timestamp_unix_ms,target,deck,solver,run,status,elapsed_ms,diag_mode,pulse_rhs,exec,freq_mhz,abs_res,rel_res,diag_spread,sin_rel_res
bench_csv:1714212345678,host,corpus/dipole-freesp-51seg.nec,hallen,1,ok,19,hallen,Nec2,cpu,14.200000,2.931358e-8,3.479257e-7,1.000000e0,0.000000e0
```

When `--bench-format json` is enabled, one JSON object per solved frequency point is emitted to stderr with the same fields under a `bench_json:` prefix.

The relative residual is defined as:
$$\mathrm{rel\_res} = \frac{\lVert Ax-b\rVert_2}{\lVert b\rVert_2}$$

## Examples

### Basic dipole run (Hallén, default)

```bash
fnec dipole.nec
```

### Explicit solver selection

```bash
fnec --solver hallen dipole.nec
```

### Execution backend selection

```bash
fnec --exec cpu dipole.nec
fnec --exec hybrid dipole.nec
fnec --exec gpu dipole.nec
```

`cpu` uses parallel CPU kernels. `hybrid` runs split-lane FR scheduling (CPU-parallel lane + GPU-candidate lane); GPU-candidate points currently fall back to CPU with diagnostics. `gpu` currently falls back to CPU with explicit diagnostics. When `--exec` is omitted the native profile auto-selects the best mode via a quick startup probe.

### Experimental pulse mode (diagnostic only)

```bash
fnec --solver pulse --pulse-rhs nec2 dipole.nec
```

### Sinusoidal mode with custom fallback threshold

```bash
fnec --solver sinusoidal --sin-fallback-rel-max 5e-3 dipole.nec
```

Overrides the default `1e-2` relative residual threshold. If the sinusoidal solve exceeds the budget the solver falls back to Hallén and reports `SOLVER_MODE sinusoidal->hallen(residual)`.

### EX type 3 with I4-divisor semantics

```bash
fnec --ex3-i4-mode divide-by-i4 dipole.nec
```

Enables experimental source normalisation using I4 as the divisor when `I4>0`. Default `legacy` mode treats type 3 the same as type 0.

### Frequency sweep via external config file

```bash
fnec --sweep-config sweep.toml dipole.nec
```

`sweep.toml`:

```toml
[frequency]
start_mhz = 14.0
end_mhz   = 30.0
step_mhz  = 0.5
```

Overrides the deck's `FR` card. See [`examples/sweep-spec.toml`](../examples/sweep-spec.toml) for the full format.

### Variable template substitution

```bash
fnec --vars params.toml template.nec
```

`params.toml`:

```toml
HALF_LEN = "5.19"
RADIUS   = "0.001"
FREQ_MHZ = "14.2"
```

`template.nec`:

```
GW 1 51 0 0 -$HALF_LEN 0 0 $HALF_LEN $RADIUS
GE
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 $FREQ_MHZ 0.0
EN
```

JSON vars files are also accepted:

```bash
fnec --vars params.json template.nec
```

An undefined `$VAR` token causes a non-zero exit with a diagnostic on stderr.

### Resonance targeting (binary search)

```bash
fnec sweep --resonance examples/resonance-search.nec.toml
```

Finds the wire length at which feedpoint reactance crosses zero (series resonance).

### Machine-readable JSON output

```bash
fnec --output-format json dipole.nec
```

Writes a JSON array to stdout — one record per solved frequency point — and keeps diagnostics on stderr.

### Benchmark instrumentation

```bash
fnec --bench dipole.nec
fnec --bench --bench-format csv dipole.nec
fnec --bench --bench-format json dipole.nec
```

Prints per-solve timing and residual diagnostics. CSV/JSON machine-readable lines go to stderr with `bench_csv:` / `bench_json:` prefixes.

### GPU far-field acceleration

```bash
fnec --exec gpu dipole.nec
```

`--exec gpu` dispatches the radiation-pattern far-field and Z-matrix-fill
kernels through real wgpu compute shaders when a wgpu adapter is available,
falling back to CPU otherwise. The dense linear solve still runs on CPU
(GPU-resident solve is tracked as roadmap item PH7-CHK-003). The legacy
`--gpu-fr` flag — which only ran a CPU computation labelled as GPU — was
removed in favour of this real GPU path (PH7-CHK-001).

### Compatibility flag placeholder

```bash
fnec --allow-noncollinear-hallen dipole.nec
```

Accepted for compatibility but silently ignored in Phase 1 — the hard-fail guardrail remains active regardless.

### Minimal deck for a 14.2 MHz half-wave dipole

```
GW 1 51 0 0 -5.282 0 0 5.282 0.001
GE
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
EN
```

---

## Test Rigs

The `scripts/` directory provides benchmark and validation rigs for CI gates,
remote execution, regression tracking, and documentation hygiene.

### Local benchmark matrix

```bash
scripts/run-benchmark-matrix.sh [output.json]
```

Runs a 3×3×3 matrix (deck × solver × exec-mode) with configurable repeats.
Writes a JSON artifact with per-run `elapsed_ms` and per-combination summary.

**Environment overrides**:

| Variable | Default |
|----------|---------|
| `FNEC_BINARY` | `./target/release/fnec` |
| `FNEC_BENCH_DECKS` | `corpus/dipole-freesp-51seg.nec corpus/dipole-ground-51seg.nec` |
| `FNEC_BENCH_SOLVERS` | `hallen pulse` |
| `FNEC_BENCH_RUNS` | `3` |
| `FNEC_BENCH_MODES` | `cpu-single cpu-multi gpu` |

### Remote SSH benchmark

```bash
scripts/pi-remote-benchmark.sh user@host
```

Syncs the workspace to a remote Linux host, builds release, and runs a
configurable benchmark sweep. Results are written as CSV in `tmp/`.

**Key env overrides**: `FNEC_BENCH_DECKS`, `FNEC_BENCH_SOLVERS`,
`FNEC_BENCH_EXECS`, `FNEC_BENCH_RUNS`, `FNEC_BENCH_HISTORY`.

```bash
# Append results automatically to a persistent history CSV
FNEC_BENCH_HISTORY="benchmarks/pi-benchmark-history.csv" \
  scripts/pi-remote-benchmark.sh user@host
```

### Compare two benchmark CSVs

```bash
scripts/pi-benchmark-compare.sh base.csv candidate.csv
scripts/pi-benchmark-compare.sh --max-delta-pct 10 base.csv candidate.csv
scripts/pi-benchmark-compare.sh --gpu-vs-cpu-max-pct 25 candidate.csv
```

Prints per-deck per-solver deltas for timing and residual diagnostics.
The `--max-delta-pct` gate fails if candidate timing regresses beyond the
threshold. The `--gpu-vs-cpu-max-pct` single-file form compares GPU vs CPU
rows within one CSV (PH5-CHK-005 G5 gate).

### Summarise a benchmark CSV

```bash
scripts/pi-benchmark-summary.sh results.csv
```

Prints average elapsed_ms grouped by (deck, solver, exec_mode), unique
`diag_mode` counts, sinusoidal fallback rows, and `diag_spread` min/max.

### Benchmark history tracking

```bash
# Append a new snapshot
scripts/pi-benchmark-history.sh append results.csv benchmarks/pi-benchmark-history.csv

# Summarise long-term trend per (deck, solver, exec_mode)
scripts/pi-benchmark-history.sh trend benchmarks/pi-benchmark-history.csv
```

The trend command shows `first_avg_ms`, `latest_avg_ms`, `delta_pct`,
`latest_timestamp_utc`, `latest_git_sha`.

### JSON regression gate

```bash
scripts/benchmark-compare-json.sh baseline.json candidate.json
scripts/benchmark-compare-json.sh --gates-file .benchmark-gates.toml base.json cand.json
```

Compares two JSON artifacts (produced by `run-benchmark-matrix.sh`) against
configurable TOML thresholds. Exit code 0 = all gates passed.

### Regression gate self-test

```bash
scripts/test-benchmark-gate.sh
```

Injects a synthetic regression and verifies that `benchmark-compare-json.sh`
correctly fires. Exit code 0 = gate logic works.

### Remote workspace test

```bash
scripts/pi-remote-workspace-check.sh user@host
```

Syncs the workspace to a remote host and runs `cargo test --workspace` there.
Requires SSH and rsync.

**Overrides**: `FNEC_TEST_COMMAND` (default: `cargo test --workspace`),
`FNEC_BOOTSTRAP_RUST` (default: 1).

### Version-bump documentation check

```bash
scripts/check-version-bump-docs.sh <base-ref> <head-ref>
```

Verifies that a version bump in `Cargo.toml` is accompanied by updates to
`docs/changelog.md`, `docs/releasenotes.md`, and `SBOM.spdx.json`.

### Documentation frontmatter validation

```bash
scripts/validate-doc-frontmatter.sh
```

Validates that every `docs/*.md` file has correct frontmatter
(`project`, `doc`, `status`, `last_updated`) and that `doc` matches the
file path.

### Documentation last-updated stamping

```bash
scripts/stamp-doc-last-updated.sh --from-git-diff <base-ref> <head-ref>
```

Updates `last_updated` to today's UTC date in all docs changed between
two git refs.

## Supported NEC cards

For the full card support matrix including field mappings and per-type details, see [docs/card-support-matrix.md](card-support-matrix.md).

Quick reference:

| Card | Support | Notes |
|------|---------|-------|
| GW | Full | Wire segment geometry definition |
| GE | Full | Geometry end; GE I1=1 infers PEC ground when no GN card is present |
| GN | Full | Ground model (type 0 = reflection coeff, type 1 = PEC image method) |
| GM | Full | Geometry move: in-place or appended transformed copies |
| GR | Full | Geometry repeat (arc repetition) |
| EX type 0 | Full | Voltage source excitation |
| EX type 1 | Partial | Implemented for `--solver pulse` as a driven-segment current source. Other solver paths still use staged portability fallback and emit the pending-semantics warning |
| EX type 2 | Partial | Accepted with staged portability behavior; currently treated like EX type 0 and emits a warning that incident-plane-wave semantics are pending |
| EX type 3 | Partial | Accepted; default `legacy` mode treats it like EX type 0 (with non-default I4 warning). Optional `--ex3-i4-mode divide-by-i4` enables experimental I4-divisor runtime semantics |
| EX type 4 | Partial | Accepted with staged portability behavior; currently treated like EX type 0 and emits a warning that segment-current semantics are pending |
| EX type 5 | Partial | Accepted with staged portability behavior; currently treated like EX type 0 and emits a warning that qdsrc semantics are pending |
| FR | Full | Linear frequency sweep over all steps |
| RP | Full | Radiation pattern calculation and report table rendering |
| LD type 0, 1, 2, 3, 4, 5 | Full | Lumped loads (series/parallel RLC, RL, RC, impedance) and distributed conductivity loads |
| TL | Partial | Lossless subset only (`type=0`); supported `NSEG` range: 0, 1, and >1 — all treated as single-section stamp (no subdivision). `segment=0` center mapping warns. Other variants warn and are ignored |
| PT | Partial | Parsed for staged portability; currently emits a deferred-support warning and is ignored at runtime |
| NT | Partial | Parsed for staged portability; currently emits a deferred-support warning and is ignored at runtime |
| EN | Full | Terminates parse |
| Other | Warning | Unknown cards print a warning and are skipped |

### Load (LD) card support

The LD card applies impedance loads to antenna segments. Supported types:

| Type | Description | Fields |
|------|-------------|--------|
| 0 | Series RLC (lumped) | F1 = R (Ω), F2 = L (H), F3 = C (F) |
| 1 | Parallel RLC (lumped) | F1 = R (Ω), F2 = L (H), F3 = C (F) |
| 2 | Series RL (lumped) | F1 = R (Ω), F2 = L (H) |
| 3 | Series RC (lumped) | F1 = R (Ω), F3 = C (F) |
| 4 | Series impedance Z = R + jX | F1 = R (Ω), F2 = X (Ω) |
| 5 | Wire conductivity (distributed) | F1 = σ (S/m) |

Example: `LD 4 1 26 26 50.0 -j100.0` applies a 50 − j100 Ω load to tag 1, segment 26.

### Transmission line (TL) card support

The TL card connects two segments with a transmission line; the current solver subset executes only lossless single-section forms, while lossy/complex models remain deferred.

**NEC field mapping** (TL I1 I2 I3 I4 I5 I6 F1 F2 F3):
- I1–I4: Segment locations (tag1, seg1, tag2, seg2)
- I5: Number of TL segments in the model (typically 1)
- I6: TL type (0 = lossless, non-zero = lossy/complex)
- F1: Characteristic impedance (Ω, default 50)
- F2: Transmission-line length (m)
- F3: Angle (°) for lossy models or velocity factor (ratio) for lossless (default 1.0)

**Solver integration**: Initial TL solver support is active for lossless cards with `type=0`. The supported `NSEG` range is `0`, `1`, and any value `> 1`; all are treated as a single-section stamp (no per-segment subdivision). `NSEG=0` is normalised to `NSEG=1` before stamping. The solver stamps a 2-port impedance model into the matrix; endpoint `segment=0` is mapped to the tag center segment with an explicit warning. Unsupported TL variants still warn and are ignored.

## Notes

- Multi-source decks (multiple EX cards) are supported; one output line per source.
- The Hallén solver rejects non-collinear and junctioned wire topologies with an explicit error. `--allow-noncollinear-hallen` is accepted for compatibility but is silently ignored in Phase 1; passing it does not change solver behaviour.
- EX type 0 is implemented across supported solver paths. EX type 1 is also implemented for `--solver pulse`; Hallen and other non-pulse modes still keep EX type 1 on the staged portability path.
- GPU acceleration (`nec_accel`) is scaffolded but not yet wired into the solve path.
- `--exec hybrid` now runs split-lane FR scheduling (CPU-parallel lane plus GPU-candidate lane) and keeps output emitted in frequency order.
- Hybrid GPU-candidate lane points are first routed through the `nec_accel` dispatch interface and currently print an explicit warning because they still run on CPU fallback until GPU kernels are wired.
- For integration testing only, setting `FNEC_ACCEL_STUB_GPU=1` enables an accelerator stub dispatch path; hybrid and gpu modes then report stub dispatch usage while still solving via CPU emulation.
- `--exec gpu` is accepted in real application runs and executes the CPU solve path today, reporting either explicit fallback diagnostics or accelerator-stub dispatch diagnostics depending on dispatch outcome.
- When `--exec` is omitted in native profile, startup emits an informational probe line to stderr showing assessed availability and the selected execution mode.

---
project: fnec-rust
doc: docs/cli-guide.md
status: living
last_updated: 2026-08-31
---

# CLI Guide — fnec (v0.17.0)

`fnec` is the command-line frontend for fnec-rust.  It reads a NEC deck file,
runs the configured solver, and prints a versioned text report to stdout
(feedpoints, currents, and RP-driven radiation pattern when requested).
Diagnostics are written to stderr.

## Synopsis

```
fnec [--solver <hallen|pulse|continuity|sinusoidal|mpie>] [--ground-solver <rcm|sommerfeld>] [--pulse-rhs <raw|nec2>] [--exec <cpu|hybrid|gpu>] [--sin-fallback-rel-max <value>] [--bench] [--bench-format <human|csv|json>] [--output-format <text|json>] [--sweep-config <file.toml>] [--vars <vars.toml|vars.json>] [--loads-config <file.toml>] [--hosts <hosts.toml>] <deck.nec>
fnec sweep --resonance <file.nec.toml>
fnec taper --sections "<dia>,<len> ..."
fnec project convert <in.toml|in.md> [out.md|out.toml]
fnec worker --stdio
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
| `--solver` | `hallen` \| `pulse` \| `continuity` \| `sinusoidal` \| `mpie` | `hallen` | MoM solver to use (see below) |
| `--ground-solver` | `rcm` \| `sommerfeld` | `rcm` | Near-ground model for a `GN` finite ground. `rcm` uses the normal-incidence scalar reflection coefficient; `sommerfeld` uses the exact Sommerfeld–Norton surface wave (PH9-CHK-006), which is what matters below ~0.1 λ. Corrects the feedpoint impedance of any **straight** wire — horizontal, vertical or tilted; bent or mixed geometry is declined with a warning and keeps the `rcm` result. Currents and patterns are unaffected either way — for those, use `--solver mpie` |
| `--pulse-rhs` | `raw` \| `nec2` | `nec2` | RHS scaling for pulse/continuity modes |
| `--exec` | `cpu` \| `hybrid` \| `gpu` | `auto` (native profile), `hybrid` (4nec2 drop-in profile) | Execution backend preference. `hybrid` uses split-lane FR scheduling (CPU-parallel lane + GPU-candidate lane) with deterministic ordered output; the GPU-candidate lane's per-frequency routing seam is not wired — a measured decision, not pending work (see the Execution modes notes) — so those points run on CPU with an explicit diagnostic. `gpu` runs real wgpu kernels for the RP far-field and — on free-space Hallén decks of ≥ 128 segments — the Z-matrix fill and GPU-resident dense solve, falling back to CPU with a diagnostic when no wgpu adapter is present. See **GPU far-field acceleration** below |
| `--sin-fallback-rel-max` | positive float | `1e-2` | Sinusoidal-only relative residual threshold for guarded fallback to Hallen. CLI flag takes precedence over `FNEC_SIN_FALLBACK_REL_MAX` env var |
| `--allow-noncollinear-hallen` | flag | off | Compatibility placeholder; accepted but silently ignored. Has no effect on solver behaviour (Phase 1). |
| `--ex3-i4-mode` | `legacy` \| `divide-by-i4` | — | **Obsolete no-op.** Accepted for backward compatibility and silently ignored; `EX` type 3 now solves as a left-hand elliptic incident plane wave regardless (PH8-CHK-002). Neither value changes any result |
| `--bench` | flag | off | Enable benchmark instrumentation plumbing (also used by the GPU benchmark timing gates) |
| `--bench-format` | `human` \| `csv` \| `json` | `human` | Emit machine-readable benchmark records to stderr as `bench_csv:` or `bench_json:` lines while keeping the normal human-readable report on stdout |
| `--output-format` | `text` \| `json` | `text` | Report format on stdout. `json` writes one JSON record per solved frequency point instead of the text report; diagnostics stay on stderr either way. Schema: `docs/json-output-schema.md` |
| `--sweep-config` | `<file.toml>` | — | Load a TOML frequency-sweep spec (range or explicit list); overrides the `FR` card frequency list for a batch solve. See `examples/sweep-spec.toml`. |
| `--vars` | `<file.toml\|file.json>` | — | Load a flat key→value map and substitute `$VAR` tokens in the deck before parsing. TOML (any extension except `.json`) and JSON flat-object files are both accepted. An undefined token causes a non-zero exit with a diagnostic. |
| `--loads-config` | `<file.toml>` | — | Load fnec-specific extended loads (Laplace-domain `Z(s) = N(s)/D(s)`) from a TOML file and stamp them on the solve alongside any `LD` cards. Hallén/pulse paths only — rejected with `--solver mpie`. See **Laplace-domain loads** below. |
| `--hosts` | `<hosts.toml>` | — | Distribute the frequency points of a sweep across SSH worker nodes listed in a TOML file (`[[worker]]` entries with `hostname` / `ssh_user` / optional `binary_path`). There is no local fallback: a missing file, no `[[worker]]` entries, or no reachable worker exits **1** with a diagnostic. See `docs/worker-deployment.md` |

### Laplace-domain loads (`--loads-config`)

NEC-2 has no card for an arbitrary rational load, so fnec reads them from a TOML
file. A Laplace load is a **series** impedance `Z(s) = N(s) / D(s)` with `s = jω`,
where `numerator`/`denominator` are the polynomial coefficients in ascending
order (`a0 + a1·s + a2·s² + …`). This generalises the built-in `LD` loads and lets
you model arbitrary matching networks, traps with parasitic resistance, or
measured/curve-fitted loads.

```toml
# A series R + L load (Z = 150 + jωL, L = 2 µH) on tag 1, segment 20.
[[laplace_load]]
tag = 1            # 0 = all tags
seg_first = 20     # 0 = all segments of the tag
# seg_last = 20    # omit or 0 = single segment
numerator   = [150.0, 2.0e-6]   # a0 + a1·s  ->  R + L·s
denominator = [1.0]
```

Equivalences (so you can cross-check against `LD`): a **series RLC**
(`R + jωL − j/(ωC)`) is `numerator = [1, R·C, L·C]`, `denominator = [0, C]`; a
flat resistor is `numerator = [R]`, `denominator = [1]`. Multiple `[[laplace_load]]`
entries are allowed. A denominator that vanishes at a swept frequency is skipped
with a warning rather than producing a non-finite matrix.

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

### `fnec taper --sections "<dia>,<len> …"`

The **Leeson step-tapered-radius correction** (D. B. Leeson, W6QHS, *Physical
Design of Yagi Antennas*, ch. 8). NEC-2-class cores — fnec included — mis-model an
element built from several tubing diameters (`--solver mpie` collapses it to the
first radius; `--solver hallen` breaks at the radius-change junctions). This
subcommand replaces a step-tapered element with the **equivalent uniform-diameter
element** (a corrected length and diameter) that has the same self-impedance near
resonance; model that uniform element in your deck instead.

Give the half-element sections from the **centre outward** as `diameter,length`
pairs in one consistent unit (the deck's units — metres):

```sh
# Book example (inches): 0.8"/0.4" dia sections, 50" each per half-element.
fnec taper --sections "0.8,50 0.4,50"
```

```
TAPER_EQUIVALENT_ELEMENT
SECTIONS 2
PHYS_HALF_LENGTH 100.000000
EQUIV_HALF_LENGTH 95.699193      # ℓ′ — half-length of the substitute cylinder
EQUIV_FULL_LENGTH 191.398386     # 2ℓ′ — full length for a GW card
EQUIV_RADIUS 0.296764            # a′ — radius for a GW card
EQUIV_DIAMETER 0.593528
KA 667.342                       # average characteristic impedance (diagnostic)
Z0 607.789
```

Build the antenna's `GW` as a **uniform** wire of full length `EQUIV_FULL_LENGTH`
and radius `EQUIV_RADIUS`. Scope: linear, essentially unloaded elements within
~±15 % of self-resonance (a no-op for uniform-diameter antennas).

### `fnec project convert <in.toml|in.md> [out.md|out.toml]`

Convert a project file between TOML and Markdown. The direction is taken from
each path's extension (`.md` = Markdown, anything else = TOML); with no output
path the converted document goes to stdout.

```sh
fnec project convert antenna.nec.toml antenna.md
fnec project convert antenna.md            # → stdout
```

### `fnec worker --stdio`

The remote solver behind `--hosts`. It reads length-prefixed task frames on
stdin and writes results on stdout, so it is **not** meant to be run by hand —
the controller spawns it over SSH on each worker node. It is documented here
because it is one of this project's four shipped artifacts and was previously
absent from both this guide and the binary's own usage text (FND-086); see
[worker-deployment.md](worker-deployment.md) for deploying it.

## Solver modes

### `hallen` (recommended for collinear wire sets)

Augmented Hallén integral equation with 8-point Gauss-Legendre quadrature and
analytic singularity subtraction.  Produces physically accurate feedpoint
impedance for thin-wire antennas when all wires are collinear with the driven
segment axis.

Bent and junctioned geometry also solves: the augmented system enforces current
continuity at wire junctions per connected conductor path, so an inverted-V, a
top-hat-loaded vertical, or an end-to-end split solves rather than erroring. Two
classes remain outside the Hallén formulation and are **warned about, not
blocked** — degree-3 (T/Y) junctions, where the Kirchhoff current split is not
modelled, and closed loops. Both solve correctly on `--solver mpie`, which the
warning names. A negative feedpoint resistance is reported as an explicit
warning, since a passive antenna cannot have one (PH9-CHK-005).

The `--allow-noncollinear-hallen` flag is a no-op: it was the opt-in for the
experimental non-collinear path before that path became the default, and is now
accepted for backward compatibility and silently ignored.

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

### `mpie` (second solver — reaches junctions, loops, near-ground currents)

Opt-in mixed-potential EFIE with a subsectional (triangle) current basis
(PH9-CHK-007). Unlike the Hallen hybrid — which folds the scalar potential into a
per-wire homogeneous term and so cannot represent it — the MPIE carries the
vector and scalar potentials separately. That lets it solve three geometry
classes the Hallen path cannot:

- **degree-3 (T/Y) junctions** — Kirchhoff's current law is satisfied by the
  junction basis itself; the Hallen path returns unphysical junction-fed
  impedance (e.g. a Y-junction reports R ≈ 8 Ω where the MPIE gives ≈ 64 Ω).
- **closed loops** — a cyclic chain with no endpoint condition.
- **near-ground currents (Sommerfeld)** — with `GN`/finite ground, the reflected
  potential kernels put the surface wave into the current solution itself, for
  **any wire above ground**: horizontal, vertical, or tilted straight wires, and
  bent geometry via the per-segment-pair reflected reaction. Only a wire that
  reaches or crosses the `z = 0` plane is rejected.

Because it keeps the scalar potential, the MPIE's absolute reactance matches
nec2c without the Hallen ~32 Ω offset (a λ/2 dipole gives ≈ 74 + j42 Ω vs
Hallen's 74 + j5 Ω).

The MPIE feeds a delta-gap at the graph node nearest the `EX`-driven segment (a
half-segment offset from NEC's segment-gap feed, vanishing under refinement). It
models geometry + voltage sources (`EX` type 0) only: `LD` loads, `TL`
transmission lines, `NT` networks, incident plane waves, and current sources are
rejected on this path.

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

RECEIVE_PATTERN
N_POINTS <n>
THETA PHI RESPONSE_DB
...

SWEEP_POINTS
N_POINTS <n>
FREQ_MHZ TAG SEG Z_RE Z_IM
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
- `SWEEP_POINTS` is emitted once, after the last per-frequency report block, on every multi-frequency text run — one row per solved point, so a sweep can be read without parsing each block. It is pinned by `apps/nec-cli/tests/report_contract.rs`; this guide simply never mentioned it (FND-086).
- `RADIATION_PATTERN` appears only when at least one `RP` card is present in the deck
- `NORMALIZED_PATTERN` appears when an `RP` card's `XNDA` field requests normalization (non-zero `X` digit); `GAIN_NORM_DB` is the total gain relative to the pattern peak (0 dB)
- `RECEIVE_PATTERN` appears only for an incident-plane-wave `EX` card with an incidence-angle sweep (NTHETA·NPHI > 1); `RESPONSE_DB` is the normalized receive response (0 dB at the sweep peak), which tracks the transmit gain pattern by reciprocity
- `NEAR_FIELD` appears only when an `NE` card is present; it lists the complex `E = (Ex, Ey, Ez)` (V/m) on the card's rectangular grid. `NEAR_H_FIELD` is the magnetic companion for an `NH` card (`H = (Hx, Hy, Hz)`, A/m)
- `CURRENTS` is controlled by a `PT` (print-control) card when present: `I1 ≤ −1` suppresses the section, `I1 = 0` prints all segments, `I1 ≥ 1` restricts output to tag `I2` and the optional segment range `I3..I4` (last `PT` card wins)

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

### Sommerfeld surface-wave ground

```bash
fnec --ground-solver sommerfeld low-dipole-gn2.nec
```

Replaces the default normal-incidence reflection-coefficient ground with the
exact Sommerfeld–Norton surface wave in the feedpoint impedance of a straight
wire (PH9-CHK-006) — the regime below ~0.1 λ where the reflection-coefficient
model misses the surface wave, including the low-height sign flip. A horizontal
λ/2 dipole at 0.05 λ over `GN 2` average ground moves from 32.9 + j9.3 Ω (`rcm`)
to 63.1 + j15.8 Ω, against nec2c's 67.3 + j52.6 Ω.

The correction is a feedpoint-impedance delta only: currents and patterns are
unchanged, and bent or mixed geometry is declined — with an explicit warning —
and keeps the `rcm` result. The low-height warning is suppressed once the
correction actually applies, since the reported `Z` then *does* model the surface
wave. For correct near-ground **currents and patterns** on arbitrary geometry
use `--solver mpie`, which assembles the reflected kernels into its Z-matrix.

### Distributed sweep across SSH workers

```bash
fnec --hosts hosts.toml frequency-sweep.nec
```

Spreads the sweep's frequency points across the `[[worker]]` nodes in
`hosts.toml`, gathering the results in frequency order. Distributed execution is
all-or-nothing — a missing file, an empty worker list, or no reachable worker
exits **1** rather than quietly solving locally. Field reference and deployment
steps: `docs/worker-deployment.md`.

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
falling back to CPU otherwise. For Hallén decks in the supported class
(free-space ground, no LD/TL cards) it also runs the **GPU-resident dense
solve** (PH7-CHK-003): the impedance matrix is filled and the system solved
entirely on the device, with only the solution vector returned. This path is
f32 (LU + iterative refinement) and matches the f64 CPU solve to ~0.01 Ω on the
reference dipole; the f64 CPU solve (`--exec cpu`) remains the accuracy
reference for corpus tolerance gates. The legacy `--gpu-fr` flag — which only
ran a CPU computation labelled as GPU — was removed in favour of this real GPU
path (PH7-CHK-001).

### Obsolete compatibility flags

```bash
fnec --allow-noncollinear-hallen --ex3-i4-mode divide-by-i4 dipole.nec
```

Both flags are accepted and silently ignored. `--allow-noncollinear-hallen` was
the opt-in for the experimental non-collinear Hallén path, which is now the
default; `--ex3-i4-mode` selected experimental `EX` type 3 semantics, which now
solve as a left-hand elliptic plane wave regardless. Neither changes any result;
they exist so older scripts keep running.

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
| GE | Full | Geometry end; `GE I1=1` infers PEC ground when no `GN` card is present |
| GM | Full | Geometry move: rotate/translate in place, or append transformed copies |
| GR | Full | Geometry repeat (successive z-axis rotation copies) |
| GN type −1 | Full | Explicit free space (same as omitting `GN`) |
| GN type 0 | Partial | Finite ground via a normal-incidence scalar reflection coefficient on the image; accurate for heights ≥ ~0.2 λ. Below 0.1 λ it misses the surface wave and warns — use `--ground-solver sommerfeld` or `--solver mpie` |
| GN type 1 | Full | Perfect-conductor (PEC) image method |
| GN type 2 | Partial | Aliases the GN 0 path by default; the true Sommerfeld surface wave comes from `--ground-solver sommerfeld` (feedpoint Z of a straight wire) or `--solver mpie` (currents and patterns, any geometry above ground) |
| EX type 0 | Full | Applied-field voltage-gap source, on every solver path |
| EX type 1 | Partial | Incident plane wave, linear polarization — solves on `--solver hallen` (receiving antenna → induced currents, no feedpoint), including degree-2 junctioned geometry. Degree-3+, closed loops and `--solver pulse` fail fast |
| EX type 2 | Partial | Incident plane wave, right-hand elliptic; solves on `--solver hallen` via the complex polarization vector |
| EX type 3 | Partial | Incident plane wave, left-hand elliptic (type 2 with opposite handedness). The legacy `--ex3-i4-mode` flag is an obsolete no-op |
| EX type 4 | Partial | Current source — solves on `--solver hallen`, forcing the specified current and reporting `Z = V/i0`; degree-2 junctions included, degree-3+ and loops fail fast |
| EX type 5 | Partial | Voltage source (current-slope discontinuity) — solves as a voltage source, same result as type 0. NEC's separate current-slope numerics are a documented non-goal |
| FR | Full | Linear frequency sweep over all steps |
| RP | Full | Radiation pattern; `XNDA` X-digit adds `NORMALIZED_PATTERN`, A-digit adds `AVERAGE_POWER_GAIN`. The N/D digits (labeling / dB-vs-ratio toggles) are deferred |
| NE / NH | Partial | Near electric / magnetic field, rectangular (`I1=0`) and spherical (`I1=1`) grids; emits `NEAR_FIELD` / `NEAR_H_FIELD` |
| LD type 0–5 | Full | Lumped loads (series/parallel RLC, RL, RC, impedance) and distributed conductivity loads. Arbitrary rational `Z(s)` loads come from `--loads-config` |
| TL type 0 | Partial | Lossless; `NSEG` 0, 1 and >1 all stamp a single 2-port section (no subdivision). `segment=0` maps to the tag centre with a warning |
| TL other | Partial | Lossy line: stamps `Z0·coth/csch(γℓ)` with `F3` = matched-line loss in dB; reduces exactly to the lossless form at 0 dB |
| PT | Partial | Print control applied at runtime: `I1 ≤ −1` suppresses current output, `I1 = 0` prints all, `I1 ≥ 1` restricts to tag `I2` / segments `I3..I4`. Last `PT` wins |
| NT | Partial | Two-port network stamped into the Z matrix (admittance → Z parameters). Malformed / singular / missing-endpoint cards warn and are skipped |
| CM / CE | Full | Comment cards; preserved in parse, ignored at runtime |
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

The TL card connects two segments with a transmission line. Both the lossless (`type = 0`) and the lossy (`type ≠ 0`) forms are stamped into the Z matrix as a single 2-port section.

**NEC field mapping** (TL I1 I2 I3 I4 I5 I6 F1 F2 F3):
- I1–I4: Segment locations (tag1, seg1, tag2, seg2)
- I5: Number of TL segments in the model (typically 1)
- I6: TL type (0 = lossless, non-zero = lossy/complex)
- F1: Characteristic impedance (Ω, default 50)
- F2: Transmission-line length (m)
- F3: Angle (°) for lossy models or velocity factor (ratio) for lossless (default 1.0)

**Solver integration**: the supported `NSEG` range is `0`, `1`, and any value `> 1`; all are treated as a single-section stamp (no per-segment subdivision), and `NSEG=0` is normalised to `NSEG=1` before stamping. Endpoint `segment=0` is mapped to the tag centre segment with an explicit warning. The lossy form (`type ≠ 0`, PH8-CHK-005) stamps `Z0·coth(γℓ)` / `Z0·csch(γℓ)` with `F3` read as matched-line loss in dB and velocity factor 1; at 0 dB it reduces exactly to the lossless stamp. Malformed cards (missing endpoint, coincident endpoints, `Z0 ≤ 0`, length `≤ 0`) warn and are skipped.

## Notes

- Multi-source decks (multiple `EX` cards) are supported; one output line per source.
- The Hallén solver handles collinear, bent, and degree-2 junctioned geometry. Degree-3 (T/Y) junctions and closed loops are *warned about*, not blocked — the warning names `--solver mpie`, which solves both correctly. `--allow-noncollinear-hallen` and `--ex3-i4-mode` are obsolete no-ops kept for backward compatibility.
- `EX` type 0 is implemented on every solver path. Types 1–5 solve on `--solver hallen`; the plane-wave types (1, 2, 3) produce induced currents for a receiving antenna rather than a feedpoint impedance. See the card table above for the per-type geometry limits.
- `--exec hybrid` runs split-lane FR scheduling (CPU-parallel lane plus GPU-candidate lane) and keeps output emitted in frequency order.
- The per-frequency GPU *routing seam* (`nec_accel::dispatch_frequency_point`) is not wired, and deliberately so: PH7-CHK-003 measured the GPU-resident dense solve at 0.04x-0.48x of the CPU at every size tested with no crossover, so routing a frequency point to it would be slower. GPU-candidate lane points print an explicit warning and run on CPU. This is separate from the real wgpu kernels, which `--exec gpu` does use — see **GPU far-field acceleration** above.
- When `--exec` is omitted in the native profile, startup emits an informational probe line to stderr: the CPU thread count, the frequency-point count, whether the **per-frequency GPU dispatch seam** will take work (it will not — see the previous bullet), and the selected execution mode. It reports that seam specifically and not GPU presence: fnec's wgpu far-field kernels do use the GPU on a machine where this line says `per_freq_gpu_dispatch=false`.

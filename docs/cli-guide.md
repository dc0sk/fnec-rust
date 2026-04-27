---
project: fnec-rust
doc: docs/cli-guide.md
status: living
last_updated: 2026-05-01
---

# CLI Guide — fnec (v0.2.0)

`fnec` is the command-line frontend for fnec-rust.  It reads a NEC deck file,
runs the configured solver, and prints a versioned text report to stdout
(feedpoints, currents, and RP-driven radiation pattern when requested).
Diagnostics are written to stderr.

## Synopsis

```
fnec [--solver <hallen|pulse|continuity|sinusoidal>] [--pulse-rhs <raw|nec2>] [--exec <cpu|hybrid|gpu>] [--allow-noncollinear-hallen] [--bench] [--bench-format <human|csv|json>] [--gpu-fr] <deck.nec>
```

Exit codes: **0** success, **1** I/O or solver error, **2** usage error.

Compatibility profile note:

- The CLI now includes a filename-steered compatibility profile scaffold for 4nec2-style external kernel replacement workflows.
- If the executable name contains `nec2dxs` or `4nec2`, default execution is steered to `--exec hybrid` unless `--exec` is explicitly provided.
- Diagnostics explicitly distinguish the two cases: "default execution path steered" vs "preserving explicit --exec=...".
- This currently changes execution-mode defaulting only; argument/output contract compatibility work remains tracked in backlog parity item `PAR-011`.
- In the native profile (normal `fnec` binary name), when `--exec` is omitted, startup now runs a quick execution probe and auto-selects the best available execution mode for the current workload shape.

## Options

| Option | Values | Default | Description |
|--------|--------|---------|-------------|
| `--solver` | `hallen` \| `pulse` \| `continuity` \| `sinusoidal` | `hallen` | MoM solver to use (see below) |
| `--pulse-rhs` | `raw` \| `nec2` | `nec2` | RHS scaling for pulse/continuity modes |
| `--exec` | `cpu` \| `hybrid` \| `gpu` | `auto` (native profile), `hybrid` (4nec2 drop-in profile) | Execution backend preference. `hybrid` uses split-lane FR scheduling (CPU-parallel lane + GPU-candidate lane) with deterministic ordered output; GPU-candidate lane points currently fall back to CPU with explicit diagnostics until GPU kernels are wired. `gpu` currently falls back to CPU kernels with explicit diagnostics |
| `--allow-noncollinear-hallen` | flag | off | Experimental: allow Hallen RHS projection on non-collinear wire topologies instead of hard fail |
| `--bench` | flag | off | Enable benchmark instrumentation plumbing (also used by GPU stub timing gates) |
| `--bench-format` | `human` \| `csv` \| `json` | `human` | Emit machine-readable benchmark records to stderr as `bench_csv:` or `bench_json:` lines while keeping the normal human-readable report on stdout |
| `--gpu-fr` | flag | off | Route RP far-field evaluation through accelerator stub path |

## Solver modes

### `hallen` (recommended for collinear wire sets)

Augmented Hallén integral equation with 8-point Gauss-Legendre quadrature and
analytic singularity subtraction.  Produces physically accurate feedpoint
impedance for thin-wire antennas when all wires are collinear with the driven
segment axis. Non-collinear topologies currently return an explicit unsupported
topology error instead of a misleading impedance.

If `--allow-noncollinear-hallen` is set, this hard-fail guardrail is bypassed
and Hallen RHS is built using feed-axis projection for all segments. This path
is experimental and can be inaccurate.

Validated result — 51-segment λ/2 dipole, 14.2 MHz:

```
74.242874 + j13.899516 Ω  (Python MoM reference: 74.23 + j13.90 Ω)
```

### `pulse` (EXPERIMENTAL)

Pulse-basis Pocklington EFIE.  **Known to diverge** from the physical solution
as segment count increases — do not use for production work.  A sinusoidal-basis
EFIE fix is tracked in `docs/backlog.md`.

### `continuity` (EXPERIMENTAL)

Same Pocklington matrix as `pulse`, but solves via a continuity-enforcing rooftop
basis transform applied per wire chain on multi-wire decks when each wire has
at least two segments. Falls back to `pulse` when topology is infeasible for
the basis transform or when residual exceeds 1e-3. Subject to the same fundamental
divergence as `pulse`.

### `sinusoidal` (EXPERIMENTAL)

Incremental milestone mode that applies a sine-tapered continuity transform on
top of the Pocklington matrix with per-wire block transforms on multi-wire decks
when each wire has at least two segments. This is not yet full NEC2 `tbf/sbf/trio` sinusoidal-basis
assembly, but it establishes a compatible stepping-stone for that implementation.
If the projected sinusoidal solve exceeds the residual budget on a single
collinear chain, the CLI falls back to `hallen` and reports
`SOLVER_MODE sinusoidal->hallen(residual)`.

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
- `RADIATION_PATTERN` appears only when at least one `RP` card is present in the deck

## Diagnostics (stderr)

A diagnostic line is always printed after the solve:

```
diag: mode=hallen pulse_rhs=Nec2 exec=cpu freq_mhz=14.200000 abs_res=3.456789e-10 rel_res=2.345678e-08 diag_spread=1.000000e0 sin_rel_res=0.000000e0
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

### Experimental pulse mode (diagnostic only)

```bash
fnec --solver pulse --pulse-rhs nec2 dipole.nec
```

### Minimal deck for a 14.2 MHz half-wave dipole

```
GW 1 51 0 0 -5.282 0 0 5.282 0.001
GE
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
EN
```

## Supported NEC cards

| Card | Support | Notes |
|------|---------|-------|
| GW | Full | Wire segment geometry definition |
| GE | Full | Geometry end; GE I1=1 infers PEC ground when no GN card is present |
| GN | Full | Ground model (type 0 = reflection coeff, type 1 = PEC image method) |
| GM | Full | Geometry move: in-place or appended transformed copies |
| GR | Full | Geometry repeat (arc repetition) |
| EX type 0 | Full | Voltage source excitation |
| EX type 3 | Partial | Accepted and currently treated like EX type 0; non-default I4 warns and normalization semantics remain pending |
| FR | Full | Linear frequency sweep over all steps |
| RP | Full | Radiation pattern calculation and report table rendering |
| LD type 0, 1, 2, 3, 4, 5 | Full | Lumped loads (series/parallel RLC, RL, RC, impedance) and distributed conductivity loads |
| TL | Partial | Lossless subset only (`type=0`, `NSEG=0/1`, `segment=0` center mapping); other variants warn and are ignored |
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

**Solver integration**: Initial TL solver support is active for lossless cards with `type=0` and `NSEG=0` or `1`. The solver stamps a 2-port impedance model into the matrix; endpoint `segment=0` is mapped to the tag center segment with an explicit warning. Unsupported TL variants still warn and are ignored.

## Notes

- Multi-source decks (multiple EX cards) are supported; one output line per source.
- The Hallén solver rejects non-collinear wire topologies by default. Use `--allow-noncollinear-hallen` only for experimental exploration.
- Only EX type 0 (voltage source) is implemented.  EX type 5 (current source / NEC `qdsrc`) is not yet supported.
- GPU acceleration (`nec_accel`) is scaffolded but not yet wired into the solve path.
- `--exec hybrid` now runs split-lane FR scheduling (CPU-parallel lane plus GPU-candidate lane) and keeps output emitted in frequency order.
- Hybrid GPU-candidate lane points are first routed through the `nec_accel` dispatch interface and currently print an explicit warning because they still run on CPU fallback until GPU kernels are wired.
- For integration testing only, setting `FNEC_ACCEL_STUB_GPU=1` enables an accelerator stub dispatch path; hybrid and gpu modes then report stub dispatch usage while still solving via CPU emulation.
- `--exec gpu` is accepted in real application runs and executes the CPU solve path today, reporting either explicit fallback diagnostics or accelerator-stub dispatch diagnostics depending on dispatch outcome.
- When `--exec` is omitted in native profile, startup emits an informational probe line to stderr showing assessed availability and the selected execution mode.

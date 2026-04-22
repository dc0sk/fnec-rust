---
project: fnec-rust
doc: docs/cli-guide.md
status: living
last_updated: 2026-04-22
---

# CLI Guide — fnec (v0.1.0)

`fnec` is the command-line frontend for fnec-rust.  It reads a NEC deck file,
runs the configured solver, and prints per-source feedpoint impedance to stdout.
Diagnostics are written to stderr.

## Synopsis

```
fnec [--solver <hallen|pulse|continuity>] [--pulse-rhs <raw|nec2>] <deck.nec>
```

Exit codes: **0** success, **1** I/O or solver error, **2** usage error.

## Options

| Option | Values | Default | Description |
|--------|--------|---------|-------------|
| `--solver` | `hallen` \| `pulse` \| `continuity` | `hallen` | MoM solver to use (see below) |
| `--pulse-rhs` | `raw` \| `nec2` | `nec2` | RHS scaling for pulse/continuity modes |

## Solver modes

### `hallen` (recommended)

Augmented Hallén integral equation with 8-point Gauss-Legendre quadrature and
analytic singularity subtraction.  Produces physically accurate feedpoint
impedance for thin-wire antennas.

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
basis transform for single linear wire chains.  Falls back to `pulse` for
multi-wire decks or when residual exceeds 1e-3.  Subject to the same fundamental
divergence as `pulse`.

## `--pulse-rhs` values

Only applies to `pulse` and `continuity` modes.

| Value | Behaviour |
|-------|-----------|
| `nec2` | Scale RHS by `−1/(λ)` — NEC2 sign/wavelength convention |
| `raw` | Use the excitation vector as-is (diagnostic use only) |

## Output format

One line per driven segment (segments with zero excitation are skipped):

```
<tag>  <seg>  <V_re>+<V_im>j  <I_re>+<I_im>j  <Z_re>+<Z_im>j
```

Columns:

| Column | Unit | Description |
|--------|------|-------------|
| tag | — | GW tag number |
| seg | — | 1-based segment index within the wire |
| V_source | V | Source voltage (`v_ex × segment_length`) |
| I | A | Current at the driven segment |
| Z_in | Ω | Feedpoint impedance (`V_source / I`) |

## Diagnostics (stderr)

A diagnostic line is always printed after the solve:

```
diag: mode=hallen pulse_rhs=Nec2 abs_res=3.456789e-10 rel_res=2.345678e-08
```

| Field | Description |
|-------|-------------|
| `mode` | Effective solver path used (may differ from `--solver` if fallback occurred) |
| `pulse_rhs` | Active `--pulse-rhs` setting |
| `abs_res` | Absolute L2 residual ‖Ax − b‖ |
| `rel_res` | Relative L2 residual ‖Ax − b‖ / ‖b‖ |

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

| Card | Support |
|------|---------|
| GW | Full |
| GE | Parsed (ground plane flag ignored) |
| EX type 0 | Full (voltage source) |
| FR | Single frequency, step count ignored |
| EN | Terminates parse |
| Other | Warning printed, skipped |

## Notes

- Multi-source decks (multiple EX cards) are supported; one output line per source.
- Only EX type 0 (voltage source) is implemented.  EX type 5 (current source / NEC `qdsrc`) is not yet supported.
- GPU acceleration (`nec_accel`) is scaffolded but not yet wired into the solve path.

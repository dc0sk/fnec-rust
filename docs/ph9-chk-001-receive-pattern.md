---
project: fnec-rust
doc: docs/ph9-chk-001-receive-pattern.md
status: living
last_updated: 2026-07-04
---

# PH9-CHK-001: incident-plane-wave receive-pattern sweep

## Requirement / change

Roadmap `PH9-CHK-001` (CP-003, PRT-003). PH8-CHK-002 solved a receiving antenna
for a **single** incidence direction. NEC's plane-wave `EX` card carries an
incidence-angle grid (NTHETA × NPHI with Δθ/Δφ); this implements the **sweep**,
producing a **receive pattern** — the antenna's response vs the wave's arrival
direction.

## Design decision: the receive-response scalar (resolved by data)

A receiving antenna driven by a plane wave has no feed port, so "response" needs a
port-free scalar. Candidates: peak induced current, total induced current power,
open-circuit voltage at a designated terminal. **Resolved empirically**: on a
z-dipole, `20·log10(peak|I|)` (normalized to the sweep peak) matches the transmit
gain pattern to **< 0.01 dB** at every angle — exactly Rayleigh–Carson
reciprocity. So the receive scalar is the **peak induced current**; no arbitrary
terminal choice is needed, and the result is reciprocity-exact.

## Implementation

- `nec_model::card::ExCard` gains `theta_inc` (F4) and `phi_inc` (F5) — the sweep
  increments Δθ/Δφ (parsed from EX fields 7/8). For a plane wave, `tag` = NTHETA,
  `segment` = NPHI, `voltage_real`/`voltage_imag` = θ0/φ0.
- `nec-cli::solve_session::plane_wave_receive_sweep` — loops the NTHETA × NPHI
  grid, solves the receiving antenna at each arrival direction (reusing the
  single-incidence `solve_plane_wave_hallen`), records the peak induced current,
  and emits the normalized response (dB, 0 at the peak).
- `nec_report` gains `ReceivePatternRow` and a `RECEIVE_PATTERN /
  THETA PHI RESPONSE_DB` section (appended when NTHETA·NPHI > 1).

## Validation (`apps/nec-cli/tests/receive_pattern.rs`)

- **Sweep shape** — z-dipole receive sweep over θ = 0..90°: endfire (θ=0) is the
  receive null, broadside (θ=90) the peak, monotonic in between.
- **Reciprocity** — the normalized receive pattern equals the normalized transmit
  θ-gain pattern (from an equivalent `RP` run) to **< 0.01 dB** at every angle.

## Test results

`cargo test --workspace`: **568 passed**, 0 failed (was 566; +2 receive-pattern
tests); clippy clean. The `RECEIVE_PATTERN` section only appears for sweep decks
(NTHETA·NPHI > 1), so existing report contracts are unaffected.

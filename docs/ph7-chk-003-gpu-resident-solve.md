---
project: fnec-rust
doc: docs/ph7-chk-003-gpu-resident-solve.md
status: living
last_updated: 2026-08-23
---

# PH7-CHK-003: GPU-resident dense Hallén solve

## Requirement / change

Roadmap checklist `PH7-CHK-003` (Phase 7): implement a GPU-resident dense linear
solve in WGSL so the filled Z-matrix from `fill_zmatrix_wgpu` is solved on-device
**without copying the full matrix back to the host**, with numerical parity vs the
CPU Hallén solve.

### Amended acceptance bar (decision 2026-06-27)

The original done-signal asked for parity "within the existing corpus impedance
tolerance gates" (0.05 Ω for `dipole-freesp-51seg`). That bar assumed an f64
solve. wgpu/WGSL compute is **f32-only** in core (no `shader-f64`), and the Hallén
solve is a *regularized normal-equations least-squares* (`(MᴴM + λI)x = Mᴴy`,
`λ=1e-8`), which squares the condition number — `λ` is below f32 machine epsilon.
An all-f32 GPU-resident solve cannot be expected to reach 0.05 Ω.

Decision (user-approved): **validate the GPU-resident solve to the established
2 Ω GPU-path tolerance** (the same bar the existing `gpu_hallen_solve.rs` G7 test
uses for the f32-fill path), and **keep the f64 CPU `solve_hallen` as the
corpus-gate (0.05 Ω) default**. The f32 precision limit is documented here and the
GPU-resident solve is an opt-in acceleration path, not the accuracy reference.

## Design

The CPU reference is `nec_solver::linear::solve_hallen` →
`solve_square_in_place` (`crates/nec_solver/src/linear.rs:672,799`). It:

1. Builds an augmented matrix `M` (`rows = N + C`, `cols = S = N + W`): rows `0..N`
   are the Z-matrix plus one per-wire homogeneous-constant column
   (`M[r][N+wire(r)] = -cos_vec[r]`); rows `N..` are endpoint (`I=0`) and junction
   (`I[a] + sign·I[b] = 0`) constraints. RHS `y[r] = rhs[r]` for `r<N`, else 0.
2. Forms the regularized normal equations `ata = MᴴM + λI`, `aty = Mᴴy`.
3. Solves `ata x = aty` by complex Gaussian elimination with partial pivoting.
4. Returns `currents = x[0..N]`.

### GPU-resident realization

Two dispatches sharing a **device-resident Z buffer** (no full-matrix copy-back):

- **Dispatch 1** — existing `zmatrix_fill.wgsl` fills `Z` (N×N, `2·N·N` f32) into a
  device storage buffer.
- **Dispatch 2** — new `hallen_normal_solve.wgsl` (single workgroup) reads the
  device `Z` buffer plus small host-uploaded metadata (per-segment `cos_vec`,
  `rhs`, `wire` index; a compact constraint-row list), and:
  - **assembles** `ata = MᴴM + λI` (`S×S`) and `aty = Mᴴy` (`S`) into a device
    scratch buffer, parallelised across the workgroup over `(i,j)` pairs. The
    `MᴴM` sum is split into the dense Z-block contribution (`r<N`, read from the
    device Z buffer) and the sparse constraint/homogeneous-column contributions;
  - **solves** in place by complex Gaussian elimination with partial pivoting and
    back-substitution (column loop with single-thread pivot selection/row swap and
    workgroup-parallel row elimination);
  - writes only the `S`-element solution vector to an output buffer.

Only the `S`-element solution is read back (`S ≤ ~64` for the corpus). The N×N
matrix stays on the device for its whole lifetime.

Complex numbers are `vec2<f32>` (`.x` real, `.y` imag), matching
`zmatrix_fill.wgsl`. Helpers: `cmul`, `cdiv`, `cconj`.

### Scope of the supported class

The GPU-resident path is wired into `--exec gpu` for the Hallén solver on decks
where the existing GPU Z-fill already applies (free-space / deferred ground,
≥ `MIN_GPU_ZMATRIX_SEGS` segments). Loads / TL stamps are host-side matrix
modifications and remain CPU-only; ground-image models remain CPU-only. When the
GPU path is unavailable or the deck is out of class, the solve falls back to the
f64 CPU `solve_hallen`.

## Tests

- `crates/nec_accel/tests/gpu_resident_solve.rs` — end-to-end parity of the fused
  fill→solve pipeline vs CPU `solve_hallen` feedpoint impedance on the 51-segment
  reference dipole, asserted ≤ 2 Ω (skips vacuously with no adapter).
- `apps/nec-cli/tests/gpu_resident_solve_cli.rs` — `--exec gpu` on 3 free-space
  corpus decks in the supported class produces feedpoint impedance within 2 Ω of
  `--exec cpu` (falls back to CPU, so Δ = 0, when no adapter is present).

## Implementation notes (as built)

The f32 path needed two numerical devices beyond a plain solve, found empirically
on real hardware:

1. **Symmetric Jacobi equilibration** of `MᴴM` (unit diagonal) before LU.
2. **Björck least-squares iterative refinement** (3 steps). The naive
   normal-equations residual `b' − A'x` is f32-noisy and *oscillates* (measured
   ΔR 4→8→4 Ω across steps). The refinement residual must be formed in the
   original M-space, `r = Mᴴ(y − Mx)`, which never squares the condition. With
   that, ΔR collapses from ~4 Ω (no refinement) to ~0.01 Ω.

The solve runs in a single workgroup; `MᴴM`, the LU factors, and all working
vectors live in device storage buffers. Systems with `S = N + W > 1024` fall back
to the CPU (fixed-size workgroup scratch).

## Test results (2026-06-27, real GPU — integrated, not discrete)

- `crates/nec_accel/tests/gpu_resident_solve.rs` — 51-seg reference dipole:
  `Z_cpu = 73.903 + j11.768 Ω`, `Z_gpu = 73.891 + j11.767 Ω` → **ΔR = 0.012 Ω,
  ΔX = 0.002 Ω** (limit 2 Ω). Deterministic across repeats.
- `apps/nec-cli/tests/gpu_resident_solve_cli.rs` — `--exec gpu` vs `--exec cpu`
  feedpoint impedance on `dipole-freesp-51seg`, `dipole-freesp-rp-51seg`,
  `dipole-freesp-gm-inplace-shifted`: **ΔR = 0.009 Ω, ΔX = 0.001 Ω** each.
- `cargo test -p nec_accel --features wgpu` clean (25 unit + 3 GPU integration).
- `cargo test --workspace` — 535 tests pass. `cargo clippy --workspace` clean.

Both deltas are far inside the amended 2 Ω bar (and inside the 0.05 Ω corpus
gate), so the f32 GPU-resident solve reaches f64-CPU quality **on the 51-segment
reference family** — while the f64 CPU solve remains the gating accuracy reference
by policy.

> **Correction (2026-08-23).** This section was headed "real discrete GPU". The
> adapter it ran on is `AMD Radeon Graphics (RADV RENOIR)`, reported by wgpu as
> `IntegratedGpu`. The measurements are real-hardware — they are not from a
> software rasterizer — but the roadmap's "at least one discrete GPU" item is
> **not** satisfied by them, and remains open.
>
> The accuracy result also does not generalise past the size it was taken at. At
> 151 segments the f32 solve is up to 7 % out and at 301 it diverges outright,
> which is why #373 added a residual gate; see the section below.

## Performance (2026-08-23) — measured, and the news is bad

The feature was accepted on accuracy alone; nothing measured whether it was
*faster*. Running the PH7-CHK-005 harness with a dense-solve section added
(`cargo run --release -p nec-cli --example gpu_crossover`, artifact
`benchmarks/real-gpu-crossover.json`):

| N segments | CPU assemble+solve | GPU-resident solve | speedup |
|-----------:|-------------------:|-------------------:|--------:|
|         32 |             212 µs |           5 782 µs |  0.04 × |
|         64 |           1 070 µs |          11 398 µs |  0.09 × |
|        128 |           6 354 µs |          45 420 µs |  0.14 × |
|        256 |         120 387 µs |         251 332 µs |  0.48 × |
|        512 |       1 329 541 µs |       **declined** |       — |

**The GPU-resident solve is slower than the CPU at every size measured, and there
is no crossover.** It trends toward parity as N grows (0.04 → 0.48) but at 512 the
#373 accuracy gate rejects it — relative residual 0.24 — so it stops producing a
usable answer before it stops being slower. It is slower where it is accurate and
inaccurate where it might have become faster.

### Why, and why more GPU will not fix it

The solve dispatches **one workgroup**:

```rust
cpass.dispatch_workgroups(1, 1, 1);   // wgpu_device.rs, the solve pass
```

with `@compute @workgroup_size(64)`. The entire LU therefore runs as 64 threads on
a **single compute unit**. The other two kernels dispatch across the device — the
Z-fill in the same function uses `((n*n)/64)` workgroups — which is why they win by
two orders of magnitude while this loses.

This is a structural ceiling, not a property of this adapter. A discrete GPU with
sixty compute units would run this kernel on one of them, so re-measuring on
bigger hardware would not change the conclusion. Making it competitive needs a
blocked/panel-factorised LU spread across workgroups, which is a different
implementation, not a tuning pass.

### What this means end to end

It explains the `--exec gpu` wall-clock. The Z-fill kernel is 100–290 × faster than
the CPU and the RP far-field kernel 56–234 × faster, yet a 10-point sweep of a
301-segment dipole runs ~4.9 s against `--exec cpu`'s ~1.7 s: the solve dominates,
and the solve is the part that loses.

**Recommendation:** keep the Z-fill and RP kernels, which earn their place
decisively; treat the GPU-resident solve as not-recommended until the LU is
re-implemented across workgroups. It is already opt-in (`--exec gpu`) and already
falls back to the CPU when the residual gate fires.

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// GPU-resident Hallén dense solve (PH7-CHK-003).
//
// Reads the device-resident Hallén Z-matrix (filled by zmatrix_fill.wgsl, never
// copied back to the host), assembles the regularized normal-equations system of
// the augmented Hallén least-squares formulation, solves it on-device by complex
// LU factorization (partial pivoting), and refines the solution with Björck
// least-squares iterative refinement — all in a single workgroup. Only the
// S-element solution vector is written back.
//
// Mirrors `nec_solver::linear::solve_hallen` / `solve_square_in_place`:
//
//   M  : R x S  augmented matrix      (R = N + C, S = N + W)
//        rows 0..N : Z-matrix + per-wire homogeneous column (-cos_vec)
//        rows N..  : endpoint (I=0) and junction (I[a]+sign*I[b]=0) constraints
//   y  : R       RHS (rhs for Z rows, 0 for constraint rows)
//   solve  min ||M x - y||  ;  currents = x[0..N]
//
// f32 numerics
// ------------
// wgpu core has no f64. The normal equations A = MᴴM square cond(M), so an f32
// solve of A x = Mᴴy alone leaves ~5 % impedance error. Two devices recover
// accuracy:
//   1. symmetric Jacobi equilibration A' = D⁻¹ A D⁻¹ (D_ii = sqrt(A_ii)) for the
//      LU factorization, and
//   2. **Björck least-squares refinement**: the residual is formed in the
//      original M-space, r = Mᴴ(y − M x), which never squares the condition, so
//      it stays accurate in f32; the LU of A' preconditions the correction.
// A plain normal-equations residual (b' − A' x) is f32-noisy and does NOT
// converge — the M-space residual is what makes this land within 2 Ω.
// The host validates to the 2 Ω GPU-path tolerance; f64 CPU solve is the
// accuracy reference.
//
// Data layout
// -----------
// binding 0 zmat (storage, read)  : 2*N*N f32  Z[r][c] @ 2*(r*N+c)
// binding 1 params(uniform)       : n, s, nc, lambda
// binding 2 aux  (storage, read)  : per-seg block then constraint block (below)
// binding 3 lu   (storage, rw)    : S×S complex — scaled A', factored in place
// binding 4 vec  (storage, rw)    : 5 complex vectors of stride R: x, gp, dx, t, out
//
// aux layout (f32):
//   per-seg  r in 0..N : [4r+0]=cos_vec[r] [4r+1]=rhs_re[r] [4r+2]=rhs_im[r] [4r+3]=wire(r)
//   constr   base=4N, ci in 0..nc :
//            [base+4ci+0]=col_a [base+4ci+1]=col_b(or -1) [base+4ci+2]=val_a [base+4ci+3]=val_b

struct Params {
    n: u32,
    s: u32,
    nc: u32,
    lambda: f32,
}

@group(0) @binding(0) var<storage, read>       zmat:   array<f32>;
@group(0) @binding(1) var<uniform>             params: Params;
@group(0) @binding(2) var<storage, read>       aux:    array<f32>;
@group(0) @binding(3) var<storage, read_write> lu:     array<f32>;
@group(0) @binding(4) var<storage, read_write> vec_:   array<f32>;

const WG: u32 = 64u;
const REFINE_STEPS: u32 = 3u;

// Fixed-size workgroup scratch; the host returns None (CPU fallback) when S
// exceeds this, so these are never indexed out of range.
const MAX_S: u32 = 1024u;
var<workgroup> dscale: array<f32, 1024>;
var<workgroup> piv: array<u32, 1024>;

// vector slots in `vec_` (stride = R = N + nc)
const SLOT_X: u32 = 0u;   // current solution (unscaled)
const SLOT_GP: u32 = 1u;  // D⁻¹ Mᴴ residual
const SLOT_DX: u32 = 2u;  // correction
const SLOT_T: u32 = 3u;   // M-space residual y − Mx
const SLOT_OUT: u32 = 4u; // unscaled result for readback

fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}
fn cdiv(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let d = b.x * b.x + b.y * b.y;
    return vec2<f32>((a.x * b.x + a.y * b.y) / d, (a.y * b.x - a.x * b.y) / d);
}
fn cconj(a: vec2<f32>) -> vec2<f32> { return vec2<f32>(a.x, -a.y); }

fn rows() -> u32 { return params.n + params.nc; }

fn z_get(r: u32, c: u32) -> vec2<f32> {
    let base = 2u * (r * params.n + c);
    return vec2<f32>(zmat[base], zmat[base + 1u]);
}
fn seg_cos(r: u32) -> f32 { return aux[4u * r]; }
fn seg_rhs(r: u32) -> vec2<f32> { return vec2<f32>(aux[4u * r + 1u], aux[4u * r + 2u]); }
fn seg_wire(r: u32) -> u32 { return u32(aux[4u * r + 3u]); }
fn con_base() -> u32 { return 4u * params.n; }
fn con_cola(ci: u32) -> u32 { return u32(aux[con_base() + 4u * ci]); }
fn con_colb_raw(ci: u32) -> f32 { return aux[con_base() + 4u * ci + 1u]; }
fn con_vala(ci: u32) -> f32 { return aux[con_base() + 4u * ci + 2u]; }
fn con_valb(ci: u32) -> f32 { return aux[con_base() + 4u * ci + 3u]; }

// Full augmented matrix entry M[r][c] (Z rows for r<N, constraint rows above).
fn m_full(r: u32, c: u32) -> vec2<f32> {
    let n = params.n;
    if r < n {
        if c < n { return z_get(r, c); }
        if seg_wire(r) == (c - n) { return vec2<f32>(-seg_cos(r), 0.0); }
        return vec2<f32>(0.0, 0.0);
    }
    let ci = r - n;
    if con_cola(ci) == c { return vec2<f32>(con_vala(ci), 0.0); }
    let cb = con_colb_raw(ci);
    if cb >= 0.0 && u32(cb) == c { return vec2<f32>(con_valb(ci), 0.0); }
    return vec2<f32>(0.0, 0.0);
}
// RHS y[r] (rhs for Z rows, 0 for constraint rows).
fn y_full(r: u32) -> vec2<f32> {
    if r < params.n { return seg_rhs(r); }
    return vec2<f32>(0.0, 0.0);
}

fn lu_get(r: u32, c: u32) -> vec2<f32> {
    let b = 2u * (r * params.s + c);
    return vec2<f32>(lu[b], lu[b + 1u]);
}
fn lu_set(r: u32, c: u32, v: vec2<f32>) {
    let b = 2u * (r * params.s + c);
    lu[b] = v.x; lu[b + 1u] = v.y;
}
fn vget(slot: u32, i: u32) -> vec2<f32> {
    let b = 2u * (slot * rows() + i);
    return vec2<f32>(vec_[b], vec_[b + 1u]);
}
fn vset(slot: u32, i: u32, v: vec2<f32>) {
    let b = 2u * (slot * rows() + i);
    vec_[b] = v.x; vec_[b + 1u] = v.y;
}

// Solve the scaled system A' z = rhs using the in-place LU + pivots, into `out`.
// (out and rhs are vector slots.) Single-invocation.
fn solve_lu(rhs_slot: u32, out_slot: u32) {
    let s = params.s;
    for (var i: u32 = 0u; i < s; i++) { vset(out_slot, i, vget(rhs_slot, i)); }
    for (var col: u32 = 0u; col < s; col++) {
        let p = piv[col];
        if p != col {
            let a = vget(out_slot, col);
            let b = vget(out_slot, p);
            vset(out_slot, col, b);
            vset(out_slot, p, a);
        }
    }
    for (var i: u32 = 0u; i < s; i++) {
        var sum = vget(out_slot, i);
        for (var j: u32 = 0u; j < i; j++) {
            sum -= cmul(lu_get(i, j), vget(out_slot, j));
        }
        vset(out_slot, i, sum);
    }
    var i = s;
    loop {
        if i == 0u { break; }
        i -= 1u;
        var sum = vget(out_slot, i);
        for (var j: u32 = i + 1u; j < s; j++) {
            sum -= cmul(lu_get(i, j), vget(out_slot, j));
        }
        vset(out_slot, i, cdiv(sum, lu_get(i, i)));
    }
}

@compute @workgroup_size(64)
fn cs_hallen_solve(@builtin(local_invocation_id) lid: vec3<u32>) {
    let tid = lid.x;
    let n = params.n;
    let s = params.s;
    let nc = params.nc;
    let r_tot = rows();

    // ---- assemble A = MᴴM (+λ) into the LU region -------------------------
    var p = tid;
    loop {
        if p >= s * s { break; }
        let i = p / s;
        let j = p % s;
        var acc = vec2<f32>(0.0, 0.0);
        for (var r: u32 = 0u; r < r_tot; r++) {
            acc += cmul(cconj(m_full(r, i)), m_full(r, j));
        }
        if i == j { acc.x += params.lambda; }
        lu_set(i, j, acc);
        p += WG;
    }
    storageBarrier();
    workgroupBarrier();

    // ---- symmetric Jacobi equilibration: A' = D⁻¹ A D⁻¹ -------------------
    var di = tid;
    loop {
        if di >= s { break; }
        dscale[di] = sqrt(max(lu_get(di, di).x, 1e-30));
        di += WG;
    }
    workgroupBarrier();
    var q = tid;
    loop {
        if q >= s * s { break; }
        let i = q / s;
        let j = q % s;
        lu_set(i, j, lu_get(i, j) * (1.0 / (dscale[i] * dscale[j])));
        q += WG;
    }
    storageBarrier();
    workgroupBarrier();

    // ---- LU factorization with partial pivoting (Doolittle) ---------------
    for (var col: u32 = 0u; col < s; col++) {
        if tid == 0u {
            var pv = col;
            var best = -1.0;
            for (var r: u32 = col; r < s; r++) {
                let v = lu_get(r, col);
                let nv = v.x * v.x + v.y * v.y;
                if nv > best { best = nv; pv = r; }
            }
            piv[col] = pv;
            if pv != col {
                for (var k: u32 = 0u; k < s; k++) {
                    let a = lu_get(col, k);
                    let b = lu_get(pv, k);
                    lu_set(col, k, b);
                    lu_set(pv, k, a);
                }
            }
        }
        storageBarrier();
        workgroupBarrier();
        let diag = lu_get(col, col);
        var row = col + 1u + tid;
        loop {
            if row >= s { break; }
            let mult = cdiv(lu_get(row, col), diag);
            lu_set(row, col, mult);
            for (var k: u32 = col + 1u; k < s; k++) {
                lu_set(row, k, lu_get(row, k) - cmul(mult, lu_get(col, k)));
            }
            row += WG;
        }
        storageBarrier();
        workgroupBarrier();
    }

    // ---- initial solve: x = D⁻¹ A'⁻¹ (D⁻¹ Mᴴy) ---------------------------
    // gp = D⁻¹ Mᴴ y   (slot GP)
    var gi = tid;
    loop {
        if gi >= s { break; }
        var acc = vec2<f32>(0.0, 0.0);
        for (var r: u32 = 0u; r < r_tot; r++) {
            acc += cmul(cconj(m_full(r, gi)), y_full(r));
        }
        vset(SLOT_GP, gi, acc * (1.0 / dscale[gi]));
        gi += WG;
    }
    storageBarrier();
    workgroupBarrier();
    if tid == 0u { solve_lu(SLOT_GP, SLOT_DX); }
    storageBarrier();
    workgroupBarrier();
    // x = D⁻¹ z
    var xi = tid;
    loop {
        if xi >= s { break; }
        vset(SLOT_X, xi, vget(SLOT_DX, xi) * (1.0 / dscale[xi]));
        xi += WG;
    }
    storageBarrier();
    workgroupBarrier();

    // ---- Björck least-squares iterative refinement ------------------------
    for (var it: u32 = 0u; it < REFINE_STEPS; it++) {
        // t = y − M x   (M-space residual, length R)
        var ti = tid;
        loop {
            if ti >= r_tot { break; }
            var acc = y_full(ti);
            for (var c: u32 = 0u; c < s; c++) {
                acc -= cmul(m_full(ti, c), vget(SLOT_X, c));
            }
            vset(SLOT_T, ti, acc);
            ti += WG;
        }
        storageBarrier();
        workgroupBarrier();
        // gp = D⁻¹ Mᴴ t
        var gj = tid;
        loop {
            if gj >= s { break; }
            var acc = vec2<f32>(0.0, 0.0);
            for (var r: u32 = 0u; r < r_tot; r++) {
                acc += cmul(cconj(m_full(r, gj)), vget(SLOT_T, r));
            }
            vset(SLOT_GP, gj, acc * (1.0 / dscale[gj]));
            gj += WG;
        }
        storageBarrier();
        workgroupBarrier();
        // dz = A'⁻¹ gp
        if tid == 0u { solve_lu(SLOT_GP, SLOT_DX); }
        storageBarrier();
        workgroupBarrier();
        // x += D⁻¹ dz
        var ui = tid;
        loop {
            if ui >= s { break; }
            vset(SLOT_X, ui, vget(SLOT_X, ui) + vget(SLOT_DX, ui) * (1.0 / dscale[ui]));
            ui += WG;
        }
        storageBarrier();
        workgroupBarrier();
    }

    // ---- copy solution to the readback slot -------------------------------
    var oi = tid;
    loop {
        if oi >= s { break; }
        vset(SLOT_OUT, oi, vget(SLOT_X, oi));
        oi += WG;
    }
    storageBarrier();
    workgroupBarrier();
}

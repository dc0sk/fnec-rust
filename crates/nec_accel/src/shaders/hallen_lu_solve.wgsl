// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// GPU-resident Hallén dense solve (PH7-CHK-003), rebuilt for FND-185.
//
// Reads the device-resident Hallén Z-matrix (filled by zmatrix_fill.wgsl, never
// copied back to the host), forms the square augmented Hallén matrix M with its
// columns scaled to unit norm, factors it by complex LU with partial pivoting,
// solves, and refines once in M-space. Only the solution vector and two residual
// norms are read back.
//
// Every hand-off between invocations is a DISPATCH BOUNDARY. The previous shader
// ran the whole solve in one workgroup and passed its matrix between invocations
// through storage buffers, separated by a storage barrier and a workgroup barrier.
// On an NVIDIA GTX 1080 Ti (Vulkan, driver 580.178.04, naga 29) a storage write
// was not reliably visible to another invocation after that barrier: invocation 0
// read an unscaled matrix entry another invocation had already rescaled, the
// pivots diverged, and the solve returned garbage (FND-185). WebGPU does
// guarantee that one dispatch's storage writes are visible to the next dispatch
// in the pass, so each phase below is its own entry point, the host encodes a
// pivot search, a row swap and an elimination dispatch per column, and no entry point reads a
// storage location another invocation of the same dispatch writes. The only
// barrier left is over WORKGROUP memory (the pivot reduction).
//
// Numerics: the class this route takes (straight wires, two free ends each) is
// square, R = N + nc = S, so M is factored directly — the normal equations
// A = MᴴM squared cond(M) (≈ 1e3 here) and left a stationary refinement close to
// its limit in f32. The LU of the column-scaled M converges with a contraction of
// about ε·cond(M) ≈ 1e-4 per refinement step (design review, f32 CPU replica).
//
// Bindings
//   0 zmat  (storage, read)  : 2*N*N f32, Z[r][c] @ 2*(r*N+c)
//   1 params(uniform)        : n, s, nc
//   2 aux   (storage, read)  : per-segment block then constraint block (below)
//   3 lu    (storage, rw)    : S×S complex, the scaled M factored in place
//   4 vec   (storage, rw)    : complex vectors of stride R+1 (slots below)
//   5 step  (uniform, dynamic offset) : col, mode for this dispatch
//
// aux layout (f32):
//   per-seg  r in 0..N : [6r+0]=cos_vec[r] [6r+1]=sin_vec[r] [6r+2]=rhs_re[r]
//                        [6r+3]=rhs_im[r] [6r+4]=cos column [6r+5]=sin column (-1 = none)
//   constr   base=6N, ci in 0..nc :
//            [base+4ci+0]=col_a [base+4ci+1]=col_b(or -1) [base+4ci+2]=val_a [base+4ci+3]=val_b

struct Params {
    n: u32,
    s: u32,
    nc: u32,
    _pad: u32,
}

struct Step {
    col: u32,
    mode: u32,
    _p0: u32,
    _p1: u32,
}

@group(0) @binding(0) var<storage, read>       zmat:   array<f32>;
@group(0) @binding(1) var<uniform>             params: Params;
@group(0) @binding(2) var<storage, read>       aux:    array<f32>;
@group(0) @binding(3) var<storage, read_write> lu:     array<f32>;
@group(0) @binding(4) var<storage, read_write> vec_:   array<f32>;
@group(0) @binding(5) var<uniform>             step:   Step;

const WG: u32 = 64u;

// vector slots in `vec_` (stride = R + 1; index R of SLOT_T carries the norms)
const SLOT_X: u32 = 0u;    // current solution (unscaled)
const SLOT_B: u32 = 1u;    // right-hand side of the next triangular solve
const SLOT_W: u32 = 2u;    // triangular-solve output (scaled correction)
const SLOT_T: u32 = 3u;    // M-space residual y − Mx; [R] = (‖y−Mx‖², ‖y‖²)
const SLOT_OUT: u32 = 4u;  // unscaled result for readback
const SLOT_D: u32 = 5u;    // column scale D_c in .x
const SLOT_PIV: u32 = 6u;  // pivot row of column c in .x

var<workgroup> red_val: array<f32, 64>;
var<workgroup> red_idx: array<u32, 64>;
var<workgroup> red_sum: array<vec2<f32>, 64>;

fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}
fn cdiv(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let d = b.x * b.x + b.y * b.y;
    return vec2<f32>((a.x * b.x + a.y * b.y) / d, (a.y * b.x - a.x * b.y) / d);
}

fn rows() -> u32 { return params.n + params.nc; }
fn stride() -> u32 { return rows() + 1u; }

fn z_get(r: u32, c: u32) -> vec2<f32> {
    let base = 2u * (r * params.n + c);
    return vec2<f32>(zmat[base], zmat[base + 1u]);
}
fn seg_cos(r: u32) -> f32 { return aux[6u * r]; }
fn seg_sin(r: u32) -> f32 { return aux[6u * r + 1u]; }
fn seg_rhs(r: u32) -> vec2<f32> { return vec2<f32>(aux[6u * r + 2u], aux[6u * r + 3u]); }
fn seg_cos_col(r: u32) -> u32 { return u32(aux[6u * r + 4u]); }
fn seg_sin_col_raw(r: u32) -> f32 { return aux[6u * r + 5u]; }
fn con_base() -> u32 { return 6u * params.n; }
fn con_cola(ci: u32) -> u32 { return u32(aux[con_base() + 4u * ci]); }
fn con_colb_raw(ci: u32) -> f32 { return aux[con_base() + 4u * ci + 1u]; }
fn con_vala(ci: u32) -> f32 { return aux[con_base() + 4u * ci + 2u]; }
fn con_valb(ci: u32) -> f32 { return aux[con_base() + 4u * ci + 3u]; }

// Augmented matrix entry M[r][c] (Z rows for r<N, constraint rows above).
fn m_full(r: u32, c: u32) -> vec2<f32> {
    let n = params.n;
    if r < n {
        if c < n { return z_get(r, c); }
        if seg_cos_col(r) == c { return vec2<f32>(-seg_cos(r), 0.0); }
        let sc = seg_sin_col_raw(r);
        if sc >= 0.0 && u32(sc) == c { return vec2<f32>(-seg_sin(r), 0.0); }
        return vec2<f32>(0.0, 0.0);
    }
    let ci = r - n;
    if con_cola(ci) == c { return vec2<f32>(con_vala(ci), 0.0); }
    let cb = con_colb_raw(ci);
    if cb >= 0.0 && u32(cb) == c { return vec2<f32>(con_valb(ci), 0.0); }
    return vec2<f32>(0.0, 0.0);
}
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
    let b = 2u * (slot * stride() + i);
    return vec2<f32>(vec_[b], vec_[b + 1u]);
}
fn vset(slot: u32, i: u32, v: vec2<f32>) {
    let b = 2u * (slot * stride() + i);
    vec_[b] = v.x; vec_[b + 1u] = v.y;
}

// D_c = ‖column c of M‖ (one invocation per column).
@compute @workgroup_size(64)
fn cs_col_scale(@builtin(global_invocation_id) gid: vec3<u32>) {
    let c = gid.x;
    if c >= params.s { return; }
    var acc = 0.0;
    for (var r: u32 = 0u; r < rows(); r++) {
        let v = m_full(r, c);
        acc += v.x * v.x + v.y * v.y;
    }
    vset(SLOT_D, c, vec2<f32>(sqrt(max(acc, 1e-30)), 0.0));
}

// lu = M D⁻¹ (one invocation per entry).
@compute @workgroup_size(64)
fn cs_build(@builtin(global_invocation_id) gid: vec3<u32>) {
    let s = params.s;
    let p = gid.x;
    if p >= s * s { return; }
    let i = p / s;
    let j = p % s;
    lu_set(i, j, m_full(i, j) * (1.0 / vget(SLOT_D, j).x));
}

// Column `step.col`: find the pivot row and record it (a reduction over WORKGROUP
// memory — the only barrier in this shader, and not a storage hand-off). One
// workgroup. The swap is the next dispatch: doing it here would write entries of
// the pivot column that other invocations of this dispatch have just read.
@compute @workgroup_size(64)
fn cs_pivot_search(@builtin(local_invocation_id) lid: vec3<u32>) {
    let tid = lid.x;
    let s = params.s;
    let col = step.col;
    var best = -1.0;
    var best_row = col;
    var r = col + tid;
    loop {
        if r >= s { break; }
        let v = lu_get(r, col);
        let nv = v.x * v.x + v.y * v.y;
        if nv > best { best = nv; best_row = r; }
        r += WG;
    }
    red_val[tid] = best;
    red_idx[tid] = best_row;
    workgroupBarrier();
    var width = WG / 2u;
    loop {
        if width == 0u { break; }
        if tid < width {
            let o = tid + width;
            // Ties go to the lower row, as the serial scan did.
            if red_val[o] > red_val[tid] || (red_val[o] == red_val[tid] && red_idx[o] < red_idx[tid]) {
                red_val[tid] = red_val[o];
                red_idx[tid] = red_idx[o];
            }
        }
        workgroupBarrier();
        width = width / 2u;
    }
    if tid == 0u { vset(SLOT_PIV, col, vec2<f32>(f32(red_idx[0]), 0.0)); }
}

// Column `step.col`: swap the pivot row into place across all columns, one
// invocation per column — each touches only its own two entries.
@compute @workgroup_size(64)
fn cs_pivot_swap(@builtin(global_invocation_id) gid: vec3<u32>) {
    let k = gid.x;
    let col = step.col;
    if k >= params.s { return; }
    let pv = u32(vget(SLOT_PIV, col).x);
    if pv == col { return; }
    let a = lu_get(col, k);
    let b = lu_get(pv, k);
    lu_set(col, k, b);
    lu_set(pv, k, a);
}

// Column `step.col`: eliminate below the pivot, one invocation per row. Each
// invocation writes only its own row and reads the pivot row, which no one
// writes in this dispatch.
@compute @workgroup_size(64)
fn cs_eliminate(@builtin(global_invocation_id) gid: vec3<u32>) {
    let s = params.s;
    let col = step.col;
    let row = col + 1u + gid.x;
    if row >= s { return; }
    let mult = cdiv(lu_get(row, col), lu_get(col, col));
    lu_set(row, col, mult);
    for (var k: u32 = col + 1u; k < s; k++) {
        lu_set(row, k, lu_get(row, k) - cmul(mult, lu_get(col, k)));
    }
}

// SLOT_B = y (the first solve's right-hand side).
@compute @workgroup_size(64)
fn cs_rhs(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= rows() { return; }
    vset(SLOT_B, i, y_full(i));
}

// SLOT_W = (LU)⁻¹ P SLOT_B, by one invocation: the triangular solves are serial,
// and a single invocation hands nothing to another.
@compute @workgroup_size(1)
fn cs_solve() {
    let s = params.s;
    for (var i: u32 = 0u; i < s; i++) { vset(SLOT_W, i, vget(SLOT_B, i)); }
    for (var col: u32 = 0u; col < s; col++) {
        let p = u32(vget(SLOT_PIV, col).x);
        if p != col {
            let a = vget(SLOT_W, col);
            let b = vget(SLOT_W, p);
            vset(SLOT_W, col, b);
            vset(SLOT_W, p, a);
        }
    }
    for (var i: u32 = 0u; i < s; i++) {
        var sum = vget(SLOT_W, i);
        for (var j: u32 = 0u; j < i; j++) {
            sum -= cmul(lu_get(i, j), vget(SLOT_W, j));
        }
        vset(SLOT_W, i, sum);
    }
    var i = s;
    loop {
        if i == 0u { break; }
        i -= 1u;
        var sum = vget(SLOT_W, i);
        for (var j: u32 = i + 1u; j < s; j++) {
            sum -= cmul(lu_get(i, j), vget(SLOT_W, j));
        }
        vset(SLOT_W, i, cdiv(sum, lu_get(i, i)));
    }
}

// x = D⁻¹ w (step.mode == 0) or x += D⁻¹ w (step.mode == 1).
@compute @workgroup_size(64)
fn cs_update_x(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= params.s { return; }
    let dz = vget(SLOT_W, i) * (1.0 / vget(SLOT_D, i).x);
    if step.mode == 0u {
        vset(SLOT_X, i, dz);
    } else {
        vset(SLOT_X, i, vget(SLOT_X, i) + dz);
    }
}

// t = y − M x into SLOT_T, and into SLOT_B as the next correction's right-hand side.
@compute @workgroup_size(64)
fn cs_residual(@builtin(global_invocation_id) gid: vec3<u32>) {
    let r = gid.x;
    if r >= rows() { return; }
    var acc = y_full(r);
    for (var c: u32 = 0u; c < params.s; c++) {
        acc -= cmul(m_full(r, c), vget(SLOT_X, c));
    }
    vset(SLOT_T, r, acc);
    vset(SLOT_B, r, acc);
}

// (‖y − Mx‖², ‖y‖²) into SLOT_T[R], which no invocation reads here.
@compute @workgroup_size(64)
fn cs_norms(@builtin(local_invocation_id) lid: vec3<u32>) {
    let tid = lid.x;
    var part = vec2<f32>(0.0, 0.0);
    var r = tid;
    loop {
        if r >= rows() { break; }
        let t = vget(SLOT_T, r);
        let y = y_full(r);
        part += vec2<f32>(t.x * t.x + t.y * t.y, y.x * y.x + y.y * y.y);
        r += WG;
    }
    red_sum[tid] = part;
    workgroupBarrier();
    if tid == 0u {
        var sum = vec2<f32>(0.0, 0.0);
        for (var i: u32 = 0u; i < WG; i++) { sum += red_sum[i]; }
        vset(SLOT_T, rows(), sum);
    }
}

// SLOT_OUT = x, for the readback.
@compute @workgroup_size(64)
fn cs_copy_out(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= params.s { return; }
    vset(SLOT_OUT, i, vget(SLOT_X, i));
}

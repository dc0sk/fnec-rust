// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// Hallén Z-matrix fill compute shader.
//
// Each thread (global_invocation_id.x) computes one element Z[row, col] of
// the N×N Hallén A-matrix:
//
//   Z[i,j] = cos(α) · ∫_{seg_j} G(R_eff) dl
//
// where G(R) = exp(−jkR)/R is the free-space scalar Green's function and
// R_eff = sqrt(|r_obs − r_src|² + a²) is the reduced kernel distance.
//
// Off-diagonal (i ≠ j): 8-point Gauss-Legendre quadrature with reduced kernel.
// Self element (i == j): 4-point GL for the smooth part + analytic log term.
//
// The output is a flat array of 2*N*N f32 values (real, imag interleaved,
// row-major): output[2*(i*N + j)] = Z[i,j].re, output[2*(i*N+j)+1] = Z[i,j].im
//
// Data layout
// -----------
// Binding 0 — segs (read-only storage): array of GpuSegment (10 × f32 AoS)
//   [mid_x, mid_y, mid_z, dir_x, dir_y, dir_z, length, radius, _pad, _pad]
// Binding 1 — uniforms (uniform, 16 B): k (f32), n (u32), _pad, _pad
// Binding 2 — output (read_write storage): [re, im, re, im, ...] row-major

struct GpuSegment {
    mid_x  : f32,
    mid_y  : f32,
    mid_z  : f32,
    dir_x  : f32,
    dir_y  : f32,
    dir_z  : f32,
    length : f32,
    radius : f32,
    _pad0  : f32,
    _pad1  : f32,
}

struct ZUniforms {
    k    : f32,
    n    : u32,
    _p0  : u32,
    _p1  : u32,
}

@group(0) @binding(0) var<storage, read>       segs     : array<GpuSegment>;
@group(0) @binding(1) var<uniform>             uniforms : ZUniforms;
@group(0) @binding(2) var<storage, read_write> output   : array<f32>;

// ---------------------------------------------------------------------------
// Gauss-Legendre nodes and weights
// ---------------------------------------------------------------------------

// 4-point GL on [-1, 1] — used for the self (smooth) part
const GL4_N = array<f32, 4>(
    -0.861136312f, -0.339981044f, 0.339981044f, 0.861136312f,
);
const GL4_W = array<f32, 4>(
    0.347854845f, 0.652145155f, 0.652145155f, 0.347854845f,
);

// 8-point GL on [-1, 1] — used for off-diagonal elements
const GL8_N = array<f32, 8>(
    -0.960289856f, -0.796666477f, -0.525532410f, -0.183434642f,
     0.183434642f,  0.525532410f,  0.796666477f,  0.960289856f,
);
const GL8_W = array<f32, 8>(
    0.101228536f, 0.222381034f, 0.313706646f, 0.362683783f,
    0.362683783f, 0.313706646f, 0.222381034f, 0.101228536f,
);

// ---------------------------------------------------------------------------
// Complex multiply: (a_re + j a_im)(b_re + j b_im)
// ---------------------------------------------------------------------------
fn cmul(a_re: f32, a_im: f32, b_re: f32, b_im: f32) -> vec2<f32> {
    return vec2<f32>(a_re * b_re - a_im * b_im, a_re * b_im + a_im * b_re);
}

// ---------------------------------------------------------------------------
// Green's function G(R) = exp(−jkR)/R  →  (cos(kR)/R, −sin(kR)/R)
// ---------------------------------------------------------------------------
fn green_k(r: f32, k: f32) -> vec2<f32> {
    let phase = -k * r;
    return vec2<f32>(cos(phase) / r, sin(phase) / r);
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------
@compute @workgroup_size(64)
fn cs_zmatrix_fill(@builtin(global_invocation_id) gid: vec3<u32>) {
    let tid = gid.x;
    let n   = uniforms.n;
    if tid >= n * n { return; }

    let i = tid / n;   // observation segment index (row)
    let j = tid % n;   // source segment index (col)
    let k = uniforms.k;

    let obs = segs[i];
    let src = segs[j];

    let cos_alpha = obs.dir_x * src.dir_x
                  + obs.dir_y * src.dir_y
                  + obs.dir_z * src.dir_z;

    let half = src.length * 0.5f;
    let a    = src.radius;

    var int_re: f32 = 0.0f;
    var int_im: f32 = 0.0f;

    if i == j {
        // ----------------------------------------------------------------
        // Self element: singularity subtraction.
        // Smooth part: GL4 over ∫ [G(R_eff) − 1/R_eff] dl
        // Analytic part: 2 ln((L/2 + R_end) / a)  (real only)
        // ----------------------------------------------------------------
        for (var m: u32 = 0u; m < 4u; m++) {
            let l     = GL4_N[m] * half;
            let r_eff = sqrt(l * l + a * a);
            let g     = green_k(r_eff, k);
            // dynamic part = G(R_eff) − 1/R_eff
            int_re += GL4_W[m] * (g.x - 1.0f / r_eff);
            int_im += GL4_W[m] *  g.y;
        }
        int_re *= half;
        int_im *= half;

        let r_end   = sqrt(half * half + a * a);
        let analytic = 2.0f * log((half + r_end) / a);
        int_re += analytic;
        // int_im unchanged (analytic term is real)

    } else {
        // ----------------------------------------------------------------
        // Off-diagonal: 8-point GL with reduced kernel.
        // r_src(t) = src.midpoint + t * half * src.direction
        // R_eff    = sqrt(|obs.mid − r_src|² + a²)
        // ----------------------------------------------------------------
        for (var m: u32 = 0u; m < 8u; m++) {
            let t = GL8_N[m] * half;

            let rx = obs.mid_x - (src.mid_x + t * src.dir_x);
            let ry = obs.mid_y - (src.mid_y + t * src.dir_y);
            let rz = obs.mid_z - (src.mid_z + t * src.dir_z);
            let r_sq  = rx * rx + ry * ry + rz * rz;
            let r_eff = sqrt(r_sq + a * a);

            let g = green_k(r_eff, k);
            int_re += GL8_W[m] * g.x;
            int_im += GL8_W[m] * g.y;
        }
        int_re *= half;
        int_im *= half;
    }

    let out_re = cos_alpha * int_re;
    let out_im = cos_alpha * int_im;

    let base = 2u * (i * n + j);
    output[base]      = out_re;
    output[base + 1u] = out_im;
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// RP far-field gain WGSL compute shader — milestone gate G3.
//
// Computes the complex far-field pattern vector (F_theta, F_phi) for one
// observation direction (theta, phi) by summing contributions from all wire
// segments.  Results are written as radiation intensity components
// (|F_theta|^2, |F_phi|^2) into the output buffer.
//
// Algorithm (matches nec_accel::gpu_kernels::far_field_components exactly):
//
//   r_hat     = (sin θ cos φ,  sin θ sin φ,  cos θ)
//   theta_hat = (cos θ cos φ,  cos θ sin φ, -sin θ)
//   phi_hat   = (-sin φ,       cos φ,         0   )
//
//   For segment n:
//     phase_arg = k · dot(mid_n, r_hat)
//     phase     = exp(j · phase_arg)   (as cos/sin pair)
//     weight    = I_n · (L_n · phase)  (complex multiply)
//     F_theta  += weight · dot(dir_n, theta_hat)
//     F_phi    += weight · dot(dir_n, phi_hat)
//
//   U_theta = |F_theta|^2,  U_phi = |F_phi|^2
//
// Data layout
// -----------
// Segments buffer (binding 0): array of Segment structs (f32, AoS):
//   mid_x, mid_y, mid_z, dir_x, dir_y, dir_z, length, _pad
//
// Currents buffer (binding 1): array of f32 pairs (re, im) per segment:
//   [I_0_re, I_0_im, I_1_re, I_1_im, ...]
//
// Uniforms (binding 2): RpUniforms
//   k           (f32) — wavenumber 2π f/c
//   theta_deg   (f32)
//   phi_deg     (f32)
//   n_segs      (u32) — number of segments
//
// Output buffer (binding 3): [u_theta_f32, u_phi_f32]

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

struct Segment {
    mid_x : f32,
    mid_y : f32,
    mid_z : f32,
    dir_x : f32,
    dir_y : f32,
    dir_z : f32,
    length : f32,
    _pad   : f32,
}

struct RpUniforms {
    k         : f32,
    theta_deg : f32,
    phi_deg   : f32,
    n_segs    : u32,
}

// ---------------------------------------------------------------------------
// Bindings
// ---------------------------------------------------------------------------

@group(0) @binding(0) var<storage, read>       segs     : array<Segment>;
@group(0) @binding(1) var<storage, read>       currents : array<f32>;
@group(0) @binding(2) var<uniform>             uniforms : RpUniforms;
@group(0) @binding(3) var<storage, read_write> output   : array<f32>;

// ---------------------------------------------------------------------------
// Helper: complex multiply  (a_re + j a_im) * (b_re + j b_im)
// ---------------------------------------------------------------------------
fn cmul(a_re: f32, a_im: f32, b_re: f32, b_im: f32) -> vec2<f32> {
    return vec2<f32>(
        a_re * b_re - a_im * b_im,
        a_re * b_im + a_im * b_re,
    );
}

// ---------------------------------------------------------------------------
// Compute entry point — single workgroup, single thread.
// This is intentionally serial-per-point; a future optimisation can use
// parallel reduction over segments.
// ---------------------------------------------------------------------------
@compute @workgroup_size(1)
fn cs_rp_farfield() {
    let pi      = 3.14159265358979323846f;
    let deg2rad = pi / 180.0f;

    let k         = uniforms.k;
    let theta_rad = uniforms.theta_deg * deg2rad;
    let phi_rad   = uniforms.phi_deg   * deg2rad;
    let n         = uniforms.n_segs;

    // Precompute spherical unit vectors.
    let st = sin(theta_rad); let ct = cos(theta_rad);
    let sp = sin(phi_rad);   let cp = cos(phi_rad);

    let rx = st * cp; let ry = st * sp; let rz = ct;           // r_hat
    let tx = ct * cp; let ty = ct * sp; let tz = -st;          // theta_hat
    let px = -sp;     let py =  cp;     let pz =  0.0f;        // phi_hat

    var ft_re: f32 = 0.0;
    var ft_im: f32 = 0.0;
    var fp_re: f32 = 0.0;
    var fp_im: f32 = 0.0;

    for (var i: u32 = 0u; i < n; i = i + 1u) {
        let seg = segs[i];
        let i_re = currents[i * 2u];
        let i_im = currents[i * 2u + 1u];

        // phase_arg = k * dot(mid, r_hat)
        let phase_arg = k * (seg.mid_x * rx + seg.mid_y * ry + seg.mid_z * rz);

        // phase = exp(j * phase_arg) = (cos, sin)
        let phase_re = cos(phase_arg);
        let phase_im = sin(phase_arg);

        // scaled_phase = L_n * phase  (real scale)
        let sp_re = seg.length * phase_re;
        let sp_im = seg.length * phase_im;

        // weight = I_n * scaled_phase  (complex multiply)
        let w = cmul(i_re, i_im, sp_re, sp_im);

        // projections
        let proj_t = seg.dir_x * tx + seg.dir_y * ty + seg.dir_z * tz;
        let proj_p = seg.dir_x * px + seg.dir_y * py + seg.dir_z * pz;

        ft_re += w.x * proj_t;
        ft_im += w.y * proj_t;
        fp_re += w.x * proj_p;
        fp_im += w.y * proj_p;
    }

    // Radiation intensity components: |F|^2
    output[0] = ft_re * ft_re + ft_im * ft_im;
    output[1] = fp_re * fp_re + fp_im * fp_im;
}

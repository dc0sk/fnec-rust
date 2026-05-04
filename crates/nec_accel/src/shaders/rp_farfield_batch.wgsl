// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// RP far-field batch compute shader — all N observation directions in one dispatch.
//
// Each thread (global_invocation_id.x == idx) computes one (theta, phi) point.
// The caller dispatches ceil(n_points / 64) workgroups.
//
// Algorithm: identical to rp_farfield.wgsl (same math, same NEC far-field
// formulation), but observation direction is read from obs_pts[idx] rather
// than a uniform, enabling a single GPU submission for an entire RP grid.
//
// Data layout
// -----------
// Binding 0 — segs (read-only storage): array of Segment (8 × f32 AoS)
// Binding 1 — currents (read-only storage): [I₀ᵣₑ, I₀ᵢₘ, I₁ᵣₑ, I₁ᵢₘ, ...]
// Binding 2 — uniforms (uniform, 16 B): k, n_segs, n_points, _pad
// Binding 3 — obs_pts (read-only storage): [θ₀, φ₀, θ₁, φ₁, ...] in degrees
// Binding 4 — output (read_write storage): [u_θ₀, u_φ₀, u_θ₁, u_φ₁, ...]

struct Segment {
    mid_x  : f32,
    mid_y  : f32,
    mid_z  : f32,
    dir_x  : f32,
    dir_y  : f32,
    dir_z  : f32,
    length : f32,
    _pad   : f32,
}

struct BatchUniforms {
    k        : f32,
    n_segs   : u32,
    n_points : u32,
    _pad     : u32,
}

@group(0) @binding(0) var<storage, read>       segs     : array<Segment>;
@group(0) @binding(1) var<storage, read>       currents : array<f32>;
@group(0) @binding(2) var<uniform>             uniforms : BatchUniforms;
@group(0) @binding(3) var<storage, read>       obs_pts  : array<f32>;
@group(0) @binding(4) var<storage, read_write> output   : array<f32>;

fn cmul(a_re: f32, a_im: f32, b_re: f32, b_im: f32) -> vec2<f32> {
    return vec2<f32>(
        a_re * b_re - a_im * b_im,
        a_re * b_im + a_im * b_re,
    );
}

@compute @workgroup_size(64)
fn cs_rp_farfield_batch(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if idx >= uniforms.n_points { return; }

    let pi      = 3.14159265358979323846f;
    let deg2rad = pi / 180.0f;

    let theta_deg = obs_pts[idx * 2u];
    let phi_deg   = obs_pts[idx * 2u + 1u];
    let theta_rad = theta_deg * deg2rad;
    let phi_rad   = phi_deg   * deg2rad;
    let k         = uniforms.k;
    let n         = uniforms.n_segs;

    let st = sin(theta_rad); let ct = cos(theta_rad);
    let sp = sin(phi_rad);   let cp = cos(phi_rad);

    let rx = st * cp; let ry = st * sp; let rz = ct;
    let tx = ct * cp; let ty = ct * sp; let tz = -st;
    let px = -sp;     let py =  cp;     let pz = 0.0f;

    var ft_re: f32 = 0.0; var ft_im: f32 = 0.0;
    var fp_re: f32 = 0.0; var fp_im: f32 = 0.0;

    for (var i: u32 = 0u; i < n; i = i + 1u) {
        let seg   = segs[i];
        let i_re  = currents[i * 2u];
        let i_im  = currents[i * 2u + 1u];

        let phase_arg = k * (seg.mid_x * rx + seg.mid_y * ry + seg.mid_z * rz);
        let phase_re  = cos(phase_arg);
        let phase_im  = sin(phase_arg);
        let sp_re     = seg.length * phase_re;
        let sp_im_v   = seg.length * phase_im;

        let w      = cmul(i_re, i_im, sp_re, sp_im_v);
        let proj_t = seg.dir_x * tx + seg.dir_y * ty + seg.dir_z * tz;
        let proj_p = seg.dir_x * px + seg.dir_y * py + seg.dir_z * pz;

        ft_re += w.x * proj_t;
        ft_im += w.y * proj_t;
        fp_re += w.x * proj_p;
        fp_im += w.y * proj_p;
    }

    output[idx * 2u]      = ft_re * ft_re + ft_im * ft_im;
    output[idx * 2u + 1u] = fp_re * fp_re + fp_im * fp_im;
}

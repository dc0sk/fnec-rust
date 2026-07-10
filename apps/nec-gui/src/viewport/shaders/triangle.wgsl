// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// GUI-CHK-001 shader spike: a hard-coded clip-space triangle with per-vertex
// color. No vertex buffer — positions/colors are indexed by @builtin(vertex_index).

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.6),
        vec2<f32>(-0.6, -0.5),
        vec2<f32>(0.6, -0.5),
    );
    var colors = array<vec3<f32>, 3>(
        vec3<f32>(0.95, 0.35, 0.30),
        vec3<f32>(0.35, 0.85, 0.45),
        vec3<f32>(0.35, 0.55, 0.95),
    );
    var out: VertexOut;
    out.clip_pos = vec4<f32>(positions[vi], 0.0, 1.0);
    out.color = colors[vi];
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}

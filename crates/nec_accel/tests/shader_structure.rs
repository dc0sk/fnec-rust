// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-193: no `const` array and no workgroup memory in fnec's shaders.
//!
//! naga (29 and 30) decorates every array type with `ArrayStride` whatever storage
//! class it lands in (gfx-rs/wgpu#7696). A constant array indexed with a loop
//! counter is copied into a function-local variable of that type, and a workgroup
//! array is one; Vulkan validation rejects both (`VUID-StandaloneSpirv-None-10684`)
//! — and a strict driver may misread them (Qualcomm Adreno renders wrong).
//! `zmatrix_fill.wgsl`'s Gauss–Legendre tables were such constants; they are
//! vectors now. `hallen_lu_solve.wgsl`'s two reductions used workgroup arrays;
//! they leave one partial per lane in storage now and the next dispatch reduces
//! them, and the layer reports nothing on a GPU solve.
//!
//! The barriers go with the workgroup memory: with none, a barrier is dead code,
//! and the solve's one rule since FND-185 — every hand-off between invocations is
//! a dispatch boundary — is checked here rather than only stated.
//!
//! CI has no GPU to run the validation layer, so the constructs are banned here.

use std::path::Path;

/// Each shader's code lines (comments stripped), with its path and line number.
fn shader_lines() -> Vec<(String, usize, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shaders");
    let mut out = Vec::new();
    let mut scanned = 0;
    for entry in std::fs::read_dir(&dir).expect("shader directory") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("wgsl") {
            continue;
        }
        scanned += 1;
        let src = std::fs::read_to_string(&path).expect("shader source");
        for (no, line) in src.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("").to_string();
            out.push((path.display().to_string(), no + 1, code));
        }
    }
    assert!(
        scanned >= 4,
        "only {scanned} shaders found — the scan is broken"
    );
    out
}

#[test]
fn no_shader_declares_a_const_array() {
    for (path, no, code) in shader_lines() {
        if code.trim_start().starts_with("const ") && code.contains("array<") {
            panic!("{path}:{no}: a const array — use vectors (FND-193): {code}");
        }
    }
}

#[test]
fn no_shader_uses_workgroup_memory_or_a_barrier() {
    for (path, no, code) in shader_lines() {
        let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        for banned in ["var<workgroup", "workgroupBarrier(", "storageBarrier("] {
            assert!(
                !compact.contains(banned),
                "{path}:{no}: `{banned}` — reduce through per-lane partials in storage and \
                 hand off at a dispatch boundary (FND-193, FND-185): {code}"
            );
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-193: no `const` array in fnec's shaders.
//!
//! naga 29 decorates every array type with `ArrayStride` whatever storage class
//! it lands in (gfx-rs/wgpu#7696). A constant array indexed with a loop counter is
//! copied into a function-local variable of that type, which Vulkan validation
//! rejects (`VUID-StandaloneSpirv-None-10684`) — and a strict driver may misread
//! (Qualcomm Adreno renders wrong). `zmatrix_fill.wgsl`'s Gauss–Legendre tables
//! were such arrays; they are vectors now, and the validation layer's error count
//! on a GPU solve went from 3 to 2. The remaining two are workgroup arrays, which
//! need naga's own fix (gfx-rs/wgpu PR #9295).
//!
//! CI has no GPU to run the validation layer, so the construct is banned here.

use std::path::Path;

#[test]
fn no_shader_declares_a_const_array() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shaders");
    let mut scanned = 0;
    for entry in std::fs::read_dir(&dir).expect("shader directory") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("wgsl") {
            continue;
        }
        scanned += 1;
        let src = std::fs::read_to_string(&path).expect("shader source");
        for (no, line) in src.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let t = code.trim_start();
            if t.starts_with("const ") && code.contains("array<") {
                panic!(
                    "{}:{}: a const array — use vectors (FND-193): {line}",
                    path.display(),
                    no + 1
                );
            }
        }
    }
    assert!(
        scanned >= 4,
        "only {scanned} shaders found — the scan is broken"
    );
}

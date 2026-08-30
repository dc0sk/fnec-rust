// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-118 — the current drive, anchored on a solver that is neither drive.
//!
//! `EX 4` and `EX 0` disagreed by 6.5% over ground. They now agree **by
//! construction**: a current source is the unit-voltage solve rescaled by
//! `i0 / I_feed`, which is exact for a linear system. That makes the obvious
//! test — "the two drives agree" — definitional, unable to fail, and therefore
//! worthless as a gate.
//!
//! So the anchor is the **MPIE solver**: a different kernel with a different
//! ground model, validated separately against `nec2c`. Both drives cannot drift
//! together past it.
//!
//! Measured on this geometry: MPIE R = 91.208. Before the fix the current drive
//! sat 5.45% away and this test fails; after, 1.16% and it passes. A gate that
//! fails before and passes after is sabotage-verified by construction.
//!
//! **Reactance is deliberately not gated here.** Hallén gives X ≈ 13.6 against
//! MPIE's ≈ 44.7 at 0.024 λ, and that gap is the reflection-coefficient
//! systematic the whole Hallén ground path carries — it is drive-INDEPENDENT, so
//! `EX 0` would fail such a gate too. Gating it would be gating a different
//! defect under this one's name.

use std::process::Command;

fn feedpoint_r(deck: &str, extra: &[&str]) -> f64 {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    for a in extra {
        cmd.arg(a);
    }
    let out = cmd
        .arg(root.join("corpus").join(deck))
        .output()
        .unwrap_or_else(|e| panic!("run fnec on {deck}: {e}"));
    assert!(
        out.status.success(),
        "{deck} must solve: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            if c.len() != 8 || c[0].parse::<u32>().is_err() {
                return None;
            }
            c[6].parse::<f64>().ok()
        })
        .next()
        .unwrap_or_else(|| panic!("no feedpoint row for {deck}:\n{stdout}"))
}

/// The gate. Before the fix: 86.238 against MPIE's 91.208 — 5.45%, fails.
#[test]
fn a_current_drive_over_ground_tracks_an_independent_kernel() {
    let hallen_ex4 = feedpoint_r("dipole-ex4-gn2-near-ground-51seg.nec", &[]);
    // MPIE refuses EX 4, so it is run on the voltage twin: same geometry, same
    // ground, same frequency, differing only in the drive it can accept.
    let mpie = feedpoint_r("dipole-gn2-near-ground-51seg.nec", &["--solver", "mpie"]);

    let rel = (hallen_ex4 - mpie).abs() / mpie;
    assert!(
        rel < 0.02,
        "the current drive over ground must track the MPIE kernel: Hallen EX4 \
         R = {hallen_ex4:.3}, MPIE R = {mpie:.3}, {:.2}% apart. Before FND-118 \
         this was 5.45%.",
        rel * 100.0
    );
}

/// The second rail, and it needs no other solver at all.
///
/// Lift the same antenna until the ground is far away and the current drive must
/// converge on its own free-space answer. The old solver's split decayed with
/// height — 6.5% at 0.024 λ down to 0.09% at 2 λ — which is the signature that
/// said the defect was in the ground coupling rather than in the drive.
#[test]
fn a_current_drive_far_above_ground_matches_free_space() {
    let high = feedpoint_r("dipole-ex4-gn2-high-above-ground.nec", &[]);
    let free = feedpoint_r("dipole-ex4-freesp-51seg.nec", &[]);
    let rel = (high - free).abs() / free;
    assert!(
        rel < 0.02,
        "at 2 lambda the ground is not a factor: {high:.3} against free space \
         {free:.3}, {:.2}% apart",
        rel * 100.0
    );
}

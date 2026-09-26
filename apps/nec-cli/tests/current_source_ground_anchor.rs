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
//! So the anchor is **nec2c**: a different kernel with a different ground
//! model, and not a drive of fnec's at all. It is not available in CI, so its
//! answer on the voltage twin of this deck is captured below, as the corpus does
//! with every external reference.
//!
//! The anchor used to be fnec's MPIE solver, run live (MPIE R = 91.208). Hallén
//! then sat within 1.16% of it, which looked like two kernels agreeing, and was
//! two defects cancelling: Hallén's free-end rows shortened every wire by one
//! segment (FND-156) and MPIE's resistance runs ~6% low (FND-157). With FND-156
//! fixed Hallén moved to 97.158 and nec2c answers 97.323; MPIE stayed at 91.208.
//!
//! **Reactance is gated too, now.** This header used to say Hallén's X ≈ 13.6
//! against MPIE's ≈ 44.7 was "the reflection-coefficient systematic the whole
//! Hallén ground path carries". It was not a ground effect: free-space Hallén was
//! 29 Ω low as well, for the same one-segment reason. Fixed Hallén gives 44.130
//! against nec2c's 44.149.

use std::process::Command;

fn feedpoint_z(deck: &str) -> (f64, f64) {
    feedpoint_z_with(deck, &[])
}

fn feedpoint_z_with(deck: &str, args: &[&str]) -> (f64, f64) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
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
            Some((c[6].parse::<f64>().ok()?, c[7].parse::<f64>().ok()?))
        })
        .next()
        .unwrap_or_else(|| panic!("no feedpoint row for {deck}:\n{stdout}"))
}

fn feedpoint_r(deck: &str) -> f64 {
    feedpoint_z(deck).0
}

/// nec2c 1.3.1 on `dipole-gn2-near-ground-51seg.nec` (the voltage twin: nec2c
/// takes the same geometry, ground and frequency), captured 2026-09-25 with the
/// deck's leading `CE` lines turned into `CM` and `XQ` appended.
const NEC2C_NEAR_GROUND: (f64, f64) = (97.323, 44.149);

/// The gate. Before FND-118 the current drive sat 6.5% from the voltage drive,
/// so it fails a 1.5% band around any kernel the voltage drive agrees with.
///
/// Run with `--ground-solver sommerfeld`, because nec2c's GN 2 IS Sommerfeld.
/// This used to run fnec's default reflection-coefficient model and match nec2c
/// to 0.16 Ω — a coincidence: the RCM error and the missing sin homogeneous term
/// (FND-158) cancelled. With the term restored, RCM reads 92.73 (4.7% low, the
/// model's own approximation) and Sommerfeld 96.59 + j40.23. X keeps the ~4 Ω
/// first-order pulse-basis residual every Hallén result has at 51 segments.
#[test]
fn a_current_drive_over_ground_tracks_an_independent_kernel() {
    let (r, x) = feedpoint_z_with(
        "dipole-ex4-gn2-near-ground-51seg.nec",
        &["--ground-solver", "sommerfeld"],
    );
    let (r_ref, x_ref) = NEC2C_NEAR_GROUND;
    let rel_r = (r - r_ref).abs() / r_ref;
    let rel_x = (x - x_ref).abs() / x_ref;
    assert!(
        rel_r < 0.015 && (x - x_ref).abs() < 5.5,
        "the current drive over ground must track nec2c: Hallen EX4 {r:.3} + j{x:.3}, \
         nec2c {r_ref} + j{x_ref}, R {:.2}% / X {:.2}% apart. Before FND-118 R was \
         6.5% off; before FND-156 X was 69% off.",
        rel_r * 100.0,
        rel_x * 100.0
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
    let high = feedpoint_r("dipole-ex4-gn2-high-above-ground.nec");
    let free = feedpoint_r("dipole-ex4-freesp-51seg.nec");
    let rel = (high - free).abs() / free;
    assert!(
        rel < 0.02,
        "at 2 lambda the ground is not a factor: {high:.3} against free space \
         {free:.3}, {:.2}% apart",
        rel * 100.0
    );
}

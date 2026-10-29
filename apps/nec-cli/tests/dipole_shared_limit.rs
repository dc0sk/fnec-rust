// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-315 — fnec's dipole converges to nec2c's limit; it only gets there more
//! slowly.
//!
//! On a coarse mesh fnec's reactance sits 1–5 % of ‖Z‖ from nec2c's at the same
//! mesh. Measured out to 2561 segments (2026-10-29), the gap at equal mesh
//! contracts by 0.58–0.61 per halving with no floor, and any offset between the
//! two codes' limits is at most 0.10 % of ‖Z‖ on the 0.2 mm wire; fnec's MPIE
//! joins the same value. fnec's coarse-mesh error constant is 2.5–3× nec2c's, so
//! it needs 3–4× the segments for nec2c's reactance error — a documented
//! limitation, not an offset to align out.
//!
//! What no other gate held: the shared limit. The corpus dipole's external gate
//! allows X ± 8 Ω, and `converge_contract.rs` pins fnec against itself. This
//! file holds fnec, absolutely, against nec2c 1.3.1 with `EK 0` on the 5 m,
//! 0.2 mm dipole at 161 and 641 segments (seg/radius 155 and 39, inside nec2c's
//! thin-wire regime), at 28, 29 and 30 MHz:
//!
//! - at 641, |ΔX| ≤ 0.20 Ω and |ΔR| ≤ 0.06 Ω (measured 0.129–0.153 and
//!   0.000–0.039);
//! - fnec approaches from below, ΔX(161) < ΔX(641) < 0, as at all nine meshes
//!   measured on this wire;
//! - |ΔX(641)| ≤ 0.45·|ΔX(161)| (measured 0.365–0.37 over the two halvings): the
//!   gap is still contracting.
//!
//! Together they fail a constant shift of fnec's X by +0.2 Ω (the sign check)
//! or by −0.06 Ω (the contraction), each sabotage-verified.
//!
//! The review proposed 321 and 1281 segments, whose margins are wider. One
//! 1281-segment point costs 166 s in the debug profile the suite runs in, and
//! cost grows eightfold per doubling, so the gate is one doubling coarser with
//! the shape checks above standing in for the lost margin.
//!
//! No extrapolation: a delta gap's reactance drifts slowly with the mesh in both
//! codes, so a fitted "limit" is softer than either stored point (FND-232's
//! review). `--exec cpu`, so the digits are the f64 solve's: meshes past the GPU
//! crossover would otherwise take the f32 device path.
//!
//! The MPIE rides along: it approaches from below too and sits nearer nec2c at
//! 641 (ΔX −0.045 … −0.055 Ω, from −0.063 … −0.098 at 161), so it is held to
//! |ΔX(641)| ≤ 0.10 Ω and the sign; its gap contracts more slowly (0.56–0.71 a
//! double halving), so not to the contraction.

use std::process::Command;

mod common;

/// nec2c 1.3.1, `EK 0`, at 28 / 29 / 30 MHz: `(R, X)` for 161 and 641 segments
/// (re-run 2026-10-29; the digits nec2c prints).
const NEC2C_161: [(f64, f64); 3] = [(63.540, -66.859), (70.730, -10.408), (78.722, 46.063)];
const NEC2C_641: [(f64, f64); 3] = [(63.495, -66.704), (70.739, -10.289), (78.805, 46.193)];

/// `(R, X)` at 28, 29 and 30 MHz from fnec's JSON records.
fn fnec(segments: u32, solver: &str) -> Vec<(f64, f64)> {
    let deck = common::TempDeck::new(
        &format!(
            "fnec-shared-limit-{solver}-{segments}-{}.nec",
            std::process::id()
        ),
        &format!(
            "CE\nGW 1 {segments} 0 0 -2.5 0 0 2.5 0.0002\nGE 0\nEX 0 1 {} 0 1 0\n\
             FR 0 3 0 0 28 1\nEN\n",
            segments.div_ceil(2)
        ),
    );
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args([
            "--solver",
            solver,
            "--exec",
            "cpu",
            "--output-format",
            "json",
        ])
        .arg(&*deck)
        .output()
        .expect("run fnec");
    assert!(
        out.status.success(),
        "{solver} {segments}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let recs: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("JSON records");
    assert_eq!(recs.len(), 3, "{recs:?}");
    recs.iter()
        .zip([28.0, 29.0, 30.0])
        .map(|(r, f)| {
            assert_eq!(r["freq_mhz"].as_f64(), Some(f), "{r}");
            (r["z_re"].as_f64().unwrap(), r["z_im"].as_f64().unwrap())
        })
        .collect()
}

/// `|ΔX| ≤ x_bound` and `|ΔR| ≤ 0.06 Ω` against nec2c at 641 segments.
fn within_the_bound(what: &str, f: usize, z: (f64, f64), n: (f64, f64), x_bound: f64) -> f64 {
    let (dr, dx) = (z.0 - n.0, z.1 - n.1);
    assert!(
        dx.abs() <= x_bound && dr.abs() <= 0.06,
        "{f} MHz, {what} at 641 segments: {z:?} vs nec2c {n:?} (ΔR {dr:.4}, ΔX {dx:.4})"
    );
    dx
}

#[test]
fn hallen_holds_nec2c_s_limit_and_the_gap_still_closes() {
    let (coarse, fine) = (fnec(161, "hallen"), fnec(641, "hallen"));
    for i in 0..3 {
        let f = 28 + i;
        let dx = within_the_bound("Hallén", f, fine[i], NEC2C_641[i], 0.20);
        let dx_coarse = coarse[i].1 - NEC2C_161[i].1;
        assert!(
            dx_coarse < dx && dx < 0.0,
            "{f} MHz: fnec no longer approaches from below, ΔX {dx_coarse:.4} at 161 → {dx:.4} at 641"
        );
        assert!(
            dx.abs() <= 0.45 * dx_coarse.abs(),
            "{f} MHz: the gap stopped closing, ΔX {dx_coarse:.4} at 161 → {dx:.4} at 641"
        );
    }
}

#[test]
fn the_mpie_joins_the_same_limit() {
    let (coarse, fine) = (fnec(161, "mpie"), fnec(641, "mpie"));
    for i in 0..3 {
        let f = 28 + i;
        let dx = within_the_bound("the MPIE", f, fine[i], NEC2C_641[i], 0.10);
        let dx_coarse = coarse[i].1 - NEC2C_161[i].1;
        assert!(
            dx_coarse < dx && dx < 0.0,
            "{f} MHz: the MPIE no longer approaches from below, ΔX {dx_coarse:.4} at 161 → {dx:.4} at 641"
        );
    }
}

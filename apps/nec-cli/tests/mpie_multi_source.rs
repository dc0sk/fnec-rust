// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-202 — `--solver mpie` drives every voltage source.
//!
//! The MPIE session drove the first delta gap alone, so a second `EX 0` was
//! dropped while the report still priced it from the single-source currents:
//! two parallel dipoles 3 m apart, both fed, answered 42.69 + j77.76 and
//! 15.73 − j118.01 Ω where Hallén gives 142.48 + j25.73 on both and nec2c
//! 145.37 + j34.69 — exit 0, no warning, in the CLI, the GUI and fnec_py alike
//! (one shared session). Pinned here against nec2c at two meshes, in phase and in
//! antiphase.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

fn tmp(name: &str, body: &str) -> common::TempDeck {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    common::TempDeck::new(&format!("fnec-mpiemulti-{name}-{n}"), body)
}

/// Every feedpoint row's `(tag, seg, R, X)`.
fn feedpoints(deck: &common::TempDeck) -> Vec<(u32, u32, f64, f64)> {
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "mpie", "--exec", "cpu"])
        .arg(deck)
        .output()
        .expect("run fnec");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "fnec failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
        .lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            if c.len() != 8 {
                return None;
            }
            Some((
                c[0].parse().ok()?,
                c[1].parse().ok()?,
                c[6].parse().ok()?,
                c[7].parse().ok()?,
            ))
        })
        .collect()
}

/// nec2c's impedance on both ports, `(segments per wire, second source, R, X)`:
/// its converged value, each deck meshed 27× finer (`scripts/nec2c-converged-series.py`,
/// captured 2026-10-16), where nec2c on the test's own mesh read 145.37 + j34.687
/// / 145.74 + j34.515 in phase and 12.390 + j53.128 / 12.435 + j53.625 in
/// antiphase — the antiphase X moving 85 % of the gate under nec2c's own
/// refinement (FND-257). Both meshes feed the centre: one antenna, so the error
/// shrinking with the mesh is measured on it.
const NEC2C: [(u32, f64, f64, f64); 4] = [
    (21, 1.0, 146.31, 34.425),
    (41, 1.0, 146.37, 34.432),
    (21, -1.0, 12.502, 54.405),
    (41, -1.0, 12.508, 54.484),
];

#[test]
fn two_fed_dipoles_on_the_mpie_match_nec2c_on_both_ports() {
    let mut err = std::collections::HashMap::new();
    for (n, v2, r, x) in NEC2C {
        let s = n.div_ceil(2);
        let d = tmp(
            "two.nec",
            &format!(
                "CE\nGW 1 {n} 0 0 -5.282 0 0 5.282 .001\nGW 2 {n} 3 0 -5.282 3 0 5.282 .001\nGE 0\nEX 0 1 {s} 0 1 0\nEX 0 2 {s} 0 {v2} 0\nFR 0 1 0 0 14.2 0\nEN\n"
            ),
        );
        let rows = feedpoints(&d);
        assert_eq!(
            rows.len(),
            2,
            "{n} segs, v2 {v2}: two feedpoints, got {rows:?}"
        );
        let mut worst = 0.0_f64;
        for (tag, seg, gr, gx) in &rows {
            let e = (gr - r).hypot(gx - x);
            eprintln!("{n} segs, v2 {v2}: {tag}/{seg} {gr:.4} {gx:+.4}j vs nec2c {r} {x:+}j");
            // The dropped-source answer was ~100 Ω away. Against nec2c's
            // converged value the MPIE at 21 per wire is 1.5 Ω off in phase and
            // 2.4 in antiphase, its own discretisation: refined (1 / 3 / 9 ×) it
            // reads 144.80, 145.65, 146.04 in R in phase and 52.00, 53.50, 53.99
            // in X in antiphase, heading for nec2c's limit. Hence 3 Ω, not the
            // 1.5 that held against nec2c on the same mesh.
            assert!(
                e < 3.0,
                "{n} segs, v2 {v2}: port {tag}/{seg} {gr} + j{gx} vs nec2c {r} + j{x} (|Δ| {e:.3} Ω)"
            );
            worst = worst.max(e);
        }
        // Symmetric drive on a symmetric pair: both ports see the same impedance.
        assert!(
            (rows[0].2 - rows[1].2).abs() < 1e-6 && (rows[0].3 - rows[1].3).abs() < 1e-6,
            "{n} segs, v2 {v2}: the ports differ: {rows:?}"
        );
        err.insert((n, v2.to_bits()), worst);
    }
    for v2 in [1.0_f64, -1.0] {
        assert!(
            err[&(41, v2.to_bits())] < err[&(21, v2.to_bits())],
            "v2 {v2}: the error does not shrink with the mesh"
        );
    }
}

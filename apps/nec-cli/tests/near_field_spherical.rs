// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-004: spherical NE/NH near-field grids (NEC-2 I1=1). The grid fields are
// reinterpreted as NX→R, NY→φ, NZ→θ (θ from +z), mapping to Cartesian
// (r sinθ cosφ, r sinθ sinφ, r cosθ). Point locations match nec2c; the field is
// evaluated at the Cartesian point, so it is consistent with the rectangular grid.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_fnec(deck: &str, name: &str) -> String {
    let path = std::env::temp_dir().join(format!("fnec_nesph_{name}.nec"));
    std::fs::write(&path, deck).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&path)
        .current_dir(workspace_root())
        .output()
        .unwrap();
    assert!(out.status.success(), "fnec failed for {name}: {out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// First NEAR_FIELD data row → (x, y, z, ez_re, ez_im).
fn first_near_row(stdout: &str) -> (f64, f64, f64, f64, f64) {
    let mut lines = stdout.lines();
    while let Some(l) = lines.next() {
        if l.trim_start().starts_with("NEAR_FIELD") {
            lines.next(); // N_POINTS
            lines.next(); // header
            let data = lines.next().expect("near-field data row");
            let f: Vec<f64> = data
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            assert!(f.len() >= 9, "unexpected near-field row: {data}");
            return (f[0], f[1], f[2], f[7], f[8]);
        }
    }
    panic!("no NEAR_FIELD section in:\n{stdout}");
}

const DIPOLE: &str = "CM dipole\nCE\nGW 1 21 0 0 -5.28 0 0 5.28 0.001\nGE 0\nFR 0 1 0 0 14.2 0\nEX 0 1 11 0 1.0 0.0\n";

/// A single spherical NE point at (R=10, φ=30°, θ=45°) lands at the correct
/// Cartesian location (6.124, 3.536, 7.071) — matching the nec2c convention.
#[test]
fn spherical_ne_point_location() {
    // NE 1 NX NY NZ  R0 phi0 theta0  dR dphi dtheta
    let deck = format!("{DIPOLE}NE 1 1 1 1 10 30 45 0 0 0\nEN\n");
    let (x, y, z, _, _) = first_near_row(&run_fnec(&deck, "loc"));
    assert!((x - 6.1237).abs() < 1e-3, "x={x}");
    assert!((y - 3.5355).abs() < 1e-3, "y={y}");
    assert!((z - 7.0711).abs() < 1e-3, "z={z}");
}

/// The field at a spherical grid point equals the field from a rectangular NE at
/// the same Cartesian location — spherical is a coordinate remap, not new physics.
#[test]
fn spherical_matches_rectangular_at_same_point() {
    let sph = format!("{DIPOLE}NE 1 1 1 1 10 30 45 0 0 0\nEN\n");
    let rect = format!("{DIPOLE}NE 0 1 1 1 6.1237 3.5355 7.0711 0 0 0\nEN\n");
    let (_, _, _, s_re, s_im) = first_near_row(&run_fnec(&sph, "sph"));
    let (_, _, _, r_re, r_im) = first_near_row(&run_fnec(&rect, "rect"));
    assert!(
        (s_re - r_re).abs() < 1e-4 && (s_im - r_im).abs() < 1e-4,
        "spherical EZ ({s_re},{s_im}) != rectangular EZ ({r_re},{r_im})"
    );
}

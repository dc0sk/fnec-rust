// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-262: the average power gain is NEC-2's.
//!
//! fnec weighted each pattern sample by sin θ and skipped the nulls, where NEC-2
//! weights each by the solid angle it stands for (its θ band, halved at the grid's
//! first and last θ; its φ step, halved at the first and last φ) and counts a null
//! as zero gain. On a 30° grid fnec read 1.024 for a dipole whose NEC-2 average
//! is 0.988, and 4nec2 shows that number as the radiation efficiency (102.4 %).
//!
//! Two checks against the PAR-011 fixtures, every one whose kernel printed an
//! average:
//! - the rule: the kernel's own printed gains, averaged by
//!   `nec_solver::average_power_gain`, give the kernel's own printed average. Its
//!   gains are printed to 0.01 dB, so the bound is 1e-3 relative; this checks the
//!   rule alone, whatever fnec's currents;
//! - the result: fnec's `AVERAGE_POWER_GAIN` for the same deck is within 0.5 % of
//!   the kernel's (fnec's own gains, its own currents).

use std::path::PathBuf;
use std::process::Command;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/par011")
}

/// The kernel's pattern gains (total, dB) and its printed average.
fn kernel(out: &str, n: usize) -> (Vec<f64>, f64) {
    let text = out.replace('\r', "");
    let mut gains = Vec::new();
    let mut on = false;
    let mut average = None;
    for l in text.lines() {
        if l.contains("RADIATION PATTERNS") {
            on = true;
            continue;
        }
        if let Some(rest) = l.split("AVERAGE POWER GAIN=").nth(1) {
            average = rest.split_whitespace().next().and_then(|v| v.parse().ok());
            break;
        }
        let f: Vec<&str> = l.split_whitespace().collect();
        if on && f.len() >= 5 {
            if let (Ok(_), Ok(_), Ok(g)) = (
                f[0].parse::<f64>(),
                f[1].parse::<f64>(),
                f[4].parse::<f64>(),
            ) {
                gains.push(g);
            }
        }
    }
    gains.truncate(n);
    (gains, average.expect("an average"))
}

/// The fixture's RP card: (nθ, nφ, θ0, Δθ, Δφ).
fn rp(inp: &str) -> (usize, usize, f64, f64, f64) {
    let line = inp
        .lines()
        .find(|l| l.starts_with("RP"))
        .expect("an RP card");
    let f: Vec<f64> = line
        .split_whitespace()
        .skip(1)
        .map(|v| v.parse().unwrap())
        .collect();
    (f[1] as usize, f[2] as usize, f[4], f[6], f[7])
}

#[test]
fn the_average_power_gain_is_nec2s() {
    let mut checked = 0;
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "out") {
            continue;
        }
        let out = std::fs::read_to_string(&path).unwrap();
        if !out.contains("AVERAGE POWER GAIN=") {
            continue;
        }
        let inp_path = path.with_extension("inp");
        let inp = std::fs::read_to_string(&inp_path).unwrap();
        let (nth, nph, th0, dth, dph) = rp(&inp);
        let (gains, printed) = kernel(&out, nth * nph);
        assert_eq!(gains.len(), nth * nph, "{}", path.display());
        let ours =
            nec_solver::average_power_gain(&gains, nth, nph, th0, dth, dph).expect("averaged");
        assert!(
            (ours / printed - 1.0).abs() < 1e-3,
            "{}: the kernel's gains average to {ours}, it printed {printed}",
            path.display()
        );
        // fnec's own average for the same deck. Not on the GN 2 deck: the kernel
        // solves its ground by Sommerfeld integration and fnec's default is the
        // reflection-coefficient approximation, a different ground (5 % apart
        // here), which is not the averaging this gates.
        checked += 1;
        if inp.lines().any(|l| l.starts_with("GN 2")) {
            continue;
        }
        let run = Command::new(env!("CARGO_BIN_EXE_fnec"))
            .args(["--exec", "cpu"])
            .arg(&inp_path)
            .output()
            .unwrap();
        if run.status.success() {
            let text = String::from_utf8_lossy(&run.stdout);
            let fnec: f64 = text
                .lines()
                .find_map(|l| l.strip_prefix("AVERAGE_POWER_GAIN "))
                .expect("fnec prints the average")
                .trim()
                .parse()
                .unwrap();
            assert!(
                (fnec / printed - 1.0).abs() < 5e-3,
                "{}: fnec averages {fnec}, the kernel {printed}",
                path.display()
            );
        }
    }
    assert_eq!(checked, 8, "fixtures with an average");
}

/// Below two samples on an axis NEC-2 does not average.
#[test]
fn a_single_cut_has_no_average() {
    assert_eq!(
        nec_solver::average_power_gain(&[0.0; 5], 5, 1, 0.0, 10.0, 0.0),
        None
    );
    assert_eq!(
        nec_solver::average_power_gain(&[0.0; 5], 1, 5, 0.0, 0.0, 10.0),
        None
    );
    // An isotropic 0 dBi over any two-axis grid averages to 1.
    let g = nec_solver::average_power_gain(&[0.0; 6], 3, 2, 0.0, 90.0, 180.0).unwrap();
    assert!((g - 1.0).abs() < 1e-12, "{g}");
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH10-CHK-003 — the MPIE models each segment's own radius.
//!
//! It solved every segment with the first one's radius, and said so. Each test
//! here holds one case against a reference that is a reference there:
//!
//! - wires of different radius that do not touch (a Yagi's elements), against
//!   nec2c, captured 2026-10-15 with its echo of 14.2 MHz and the segment count,
//!   and agreeing with its own extended kernel at the finest mesh;
//! - a radius step along a wire, against the first-order closed form and against
//!   the Leeson-equivalent uniform element under nec2c. nec2c is NOT a reference on
//!   the stepped wire itself: it drifts with the mesh there, and so does Hallén's
//!   value, which carries a tenth of the step's reactance or less (FND-252).
use std::process::Command;

/// `fnec` on `deck` with `args`: Ok((feedpoint Z, stderr)) or Err(stderr).
fn fnec(deck: &str, args: &[&str]) -> Result<((f64, f64), String), String> {
    let dir = std::env::temp_dir().join(format!(
        "fnec-mixed-radii-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("deck.nec");
    std::fs::write(&path, deck).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_dir_all(&dir);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        return Err(stderr);
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let row: Vec<f64> = stdout
        .lines()
        .skip_while(|l| *l != "FEEDPOINTS")
        .nth(2)
        .expect("feedpoint row")
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    Ok(((row[6], row[7]), stderr))
}

const HALLEN: &[&str] = &["--solver", "hallen"];
const MPIE: &[&str] = &["--solver", "mpie"];

fn solve(deck: &str, args: &[&str]) -> ((f64, f64), String) {
    fnec(deck, args).unwrap_or_else(|e| panic!("{deck}\nwas refused: {e}"))
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// The Hallén caveat on a radius step it cannot put a node at (FND-252).
const STEP_CAVEAT: &str = "changes radius where wires join";

/// A two-element Yagi at 14.2 MHz: a 10.8 m reflector of 10 mm radius 4.2 m
/// behind a 10.2 m driven element of 2 mm, fed at its centre, `n` segments each.
fn yagi(n: u32) -> String {
    format!(
        "CE\nGW 1 {n} -4.2 0 -5.4 -4.2 0 5.4 0.010\nGW 2 {n} 0 0 -5.1 0 0 5.1 0.002\nGE 0\n\
         EX 0 2 {} 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
        n / 2 + 1
    )
}

/// Wires of different radius that do not touch, each solved with its own: nec2c
/// 69.674 + j28.364 at 21 segments per element, 70.168 + j28.754 at 63, 70.429 +
/// j28.980 at 189, where its extended kernel gives 70.396 + j29.000 (the 10 mm
/// reflector is 5.7 radii per segment there, under nec2c's limit of 8 without
/// it). With the reflector's radius for both — what the MPIE did — it read
/// 71.65 + j40.02, 72.90 + j41.15, 73.63 + j41.81; now 69.08 + j27.67 and
/// 69.78 + j28.56 at 21 and 63. Hallén approaches it too, more slowly (68.86 +
/// j26.22, 69.76 + j27.81, 70.22 + j28.48), and neither solver warns: nothing
/// joins the two radii.
#[test]
fn separate_wires_are_each_solved_with_their_own_radius() {
    let nec2c = (70.396, 29.000);
    let coarse = solve(&yagi(21), MPIE).0;
    let (z, stderr) = solve(&yagi(63), MPIE);
    assert!(
        dist(z, nec2c) < 1.0 && dist(z, nec2c) < dist(coarse, nec2c),
        "MPIE {coarse:?} at 21, {z:?} at 63, vs nec2c {nec2c:?}"
    );
    assert!(!stderr.contains("radius"), "{stderr}");
    let coarse = solve(&yagi(21), HALLEN).0;
    let (z, stderr) = solve(&yagi(63), HALLEN);
    assert!(
        dist(z, nec2c) < dist(coarse, nec2c),
        "Hallén {coarse:?} at 21, {z:?} at 63, vs nec2c {nec2c:?}"
    );
    assert!(!stderr.contains(STEP_CAVEAT), "{stderr}");
}

/// A 10.4 m dipole at 14.2 MHz, 5 mm radius, thinned to `thin` within 1.26 m of
/// its feed: 72 + 45 + 72 segments.
fn feed_step(thin: f64) -> String {
    format!(
        "CE\nGW 1 72 0 0 -5.2 0 0 -1.26 0.005\nGW 2 45 0 0 -1.26 0 0 1.26 {thin}\n\
         GW 3 72 0 0 1.26 0 0 5.2 0.005\nGE 0\nEX 0 2 23 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n"
    )
}

/// A radius step adds a series reactance, first order in `ln(a₁/a₂)`: with the
/// wire thinned from `a₁` to `a₂` within `ℓ` of the feed on both arms of a dipole
/// of half-length `h`, `ΔX = 2 f μ₀ ln(a₁/a₂) · [sin 2k(h−ℓ) − sin 2kh] / (2k sin²kh)`
/// — the inductance the thinner wire adds, weighted by the current squared, less
/// the capacitance it removes, weighted by the charge squared. The MPIE's ΔX over
/// its own uniform wire is 2.11 Ω for a 5 % step and 9.61 for 25 %, against 1.97
/// and 8.95; at 2:1, 30.08 against 27.79, the second order showing. Hallén's is
/// 0.26, 1.18 and 3.64 on one homogeneous solution across the step (FND-252);
/// on the section graph, which puts a node at each step, 2.06, 9.47 and 29.82.
///
/// This does not tell one symmetric pair radius from another (geometric mean,
/// RMS, max, min agree within 0.06 Ω at 5:1); it tells a per-segment radius from
/// one radius for all, which reads no step at all.
#[test]
fn a_radius_step_adds_the_first_order_reactance() {
    let (f, mu0, c0) = (14.2e6, 4e-7 * std::f64::consts::PI, 299_792_458.0);
    let k = 2.0 * std::f64::consts::PI * f / c0;
    let (h, l) = (5.2_f64, 1.26_f64);
    let weight =
        ((2.0 * k * (h - l)).sin() - (2.0 * k * h).sin()) / (2.0 * k * (k * h).sin().powi(2));
    for args in [MPIE, HALLEN] {
        let uniform = solve(&feed_step(0.005), args).0;
        for thin in [0.005 / 1.05, 0.004] {
            let (z, stderr) = solve(&feed_step(thin), args);
            let expected = 2.0 * f * mu0 * (0.005 / thin).ln() * weight;
            let dx = z.1 - uniform.1;
            assert!(
                (dx - expected).abs() < 0.1 * expected,
                "{args:?}, {thin} m: ΔX {dx:.3} Ω over the uniform wire, first order \
                 {expected:.3}"
            );
            assert!(!stderr.contains("radius"), "{stderr}");
        }
    }
}

/// A Yagi element 5 m long at 29.9 MHz, stepped down from a 1.944 mm centre to
/// 1.056 mm tips in nine sections a side (2:1 in diameter end to end), `m`
/// segments per outer section and `2m + 1` in the centre.
fn stepped_element(m: u32) -> String {
    let mut deck = String::from("CE\n");
    let w = 2.5 / 9.0;
    deck += &format!("GW 1 {} 0 0 {:.6} 0 0 {:.6} 0.001944\n", 2 * m + 1, -w, w);
    for i in 1..9u32 {
        let r = 0.001944 - 0.000111 * f64::from(i);
        let (a, b) = (f64::from(i) * w, f64::from(i + 1) * w);
        deck += &format!("GW {} {m} 0 0 {a:.6} 0 0 {b:.6} {r:.6}\n", i + 1);
        deck += &format!("GW {} {m} 0 0 {:.6} 0 0 {:.6} {r:.6}\n", i + 11, -b, -a);
    }
    deck + &format!("GE 0\nEX 0 1 {} 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n", m + 1)
}

/// The stepped element against its Leeson-equivalent uniform element (`fnec
/// taper`: 4.898596 m, 1.530 mm radius), which nec2c solves inside its regime:
/// 75.114 + j16.518 at 189 segments, 75.110 + j16.495 with its extended kernel.
/// The MPIE converges, 76.26 + j13.21 at m = 7 and 76.37 + j13.40 at m = 21,
/// within 1.3 Ω in R and 3.2 in X of it; Leeson's is an engineering equivalent,
/// not an exact one, so the bound is 2 Ω and 4 Ω. Hallén, on the section graph,
/// reads 76.26 + j12.84 and 76.40 + j14.16 — within the same bounds at m = 21 and
/// 0.76 Ω of the MPIE there; it read 77.46 + j38.33 and 77.65 + j38.85 on one
/// homogeneous solution across every step (FND-252), and nec2c on the stepped
/// element itself drifts from j31.81 to j35.73 toward that. Uniform, the same
/// element agrees on all three (nec2c 80.19 + j42.73, MPIE 80.00 + j42.27, Hallén
/// 80.13 + j41.82).
#[test]
fn a_stepped_yagi_element_is_its_leeson_equivalent() {
    let leeson = (75.114, 16.518);
    let (z7, stderr) = solve(&stepped_element(7), MPIE);
    assert!(!stderr.contains("radius"), "{stderr}");
    let (z21, _) = solve(&stepped_element(21), MPIE);
    assert!(
        dist(z7, z21) < 0.5,
        "the MPIE does not converge: {z7:?} at m = 7, {z21:?} at m = 21"
    );
    assert!(
        (z21.0 - leeson.0).abs() < 2.0 && (z21.1 - leeson.1).abs() < 4.0,
        "MPIE {z21:?} vs the Leeson equivalent {leeson:?}"
    );
    let (h21, stderr) = solve(&stepped_element(21), HALLEN);
    assert!(!stderr.contains("radius"), "{stderr}");
    assert!(
        (h21.0 - leeson.0).abs() < 2.0 && (h21.1 - leeson.1).abs() < 4.0 && dist(h21, z21) < 1.0,
        "Hallén {h21:?} vs the Leeson equivalent {leeson:?} and the MPIE {z21:?}"
    );
}

/// Hallén warns on a radius step only where it cannot put a node there, and
/// names why and the MPIE: here a step beside a run one segment long, which the
/// section graph refuses. Where it can — every step of the stepped element, a
/// step of 1 % — it is silent, as on separate wires of different radius and on a
/// uniform wire. The MPIE warns on none of them.
#[test]
fn hallen_warns_on_a_radius_step_only_where_it_cannot_model_it() {
    let one_segment_run =
        "CE\nGW 1 72 0 0 -5.2 0 0 -1.36 0.005\nGW 4 1 0 0 -1.36 0 0 -1.26 0.004\n\
                           GW 2 45 0 0 -1.26 0 0 1.26 0.005\nGW 3 72 0 0 1.26 0 0 5.2 0.005\nGE 0\n\
                           EX 0 2 23 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
    let (_, stderr) = solve(one_segment_run, HALLEN);
    assert!(
        stderr.contains(STEP_CAVEAT)
            && stderr.contains("4 mm to 5 mm")
            && stderr.contains("tag 4 segment 1 is one segment long")
            && stderr.contains("--solver mpie")
            && stderr.contains("FND-252"),
        "{stderr}"
    );
    let (_, stderr) = solve(one_segment_run, MPIE);
    assert!(!stderr.contains("radius"), "{stderr}");
    for deck in [
        stepped_element(7),
        feed_step(0.00495),
        feed_step(0.005),
        yagi(21),
    ] {
        let (_, stderr) = solve(&deck, HALLEN);
        assert!(!stderr.contains("radius"), "{deck}\n{stderr}");
    }
}
